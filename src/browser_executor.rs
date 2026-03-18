use chromiumoxide::browser::HeadlessMode;
use chromiumoxide::element::Element;
use chromiumoxide::{Browser, BrowserConfig, Page};
use futures::StreamExt;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::warn;

use crate::types::{ActionTarget, AgentAction, ElementSnapshot};

pub struct BrowserExecutor {
    page: Arc<Mutex<Page>>,
    // Giữ browser alive — dropped khi BrowserExecutor dropped
    _browser: Browser,
}

impl std::fmt::Debug for BrowserExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserExecutor").finish()
    }
}

impl BrowserExecutor {
    pub async fn new(starting_url: &str) -> anyhow::Result<Self> {
        // Chrome executable path from env — configurable for different deployment environments
        let chrome_path = std::env::var("CHROME_EXECUTABLE").unwrap_or_else(|_| {
            // Thử các path phổ biến
            for path in &[
                "/usr/bin/chromium-browser",
                "/usr/bin/chromium",
                "/opt/google/chrome/chrome",
                "/usr/bin/google-chrome",
            ] {
                if std::path::Path::new(path).exists() {
                    return path.to_string();
                }
            }
            "chromium-browser".to_string() // fallback to PATH
        });

        let headless = std::env::var("BROWSER_HEADLESS")
            .unwrap_or_else(|_| "true".to_string())
            .to_lowercase()
            != "false";

        let headless_mode = if headless {
            HeadlessMode::New
        } else {
            HeadlessMode::False
        };

        // [CRITICAL] Required flags for container deployments (DO App Platform):
        let config = BrowserConfig::builder()
            .chrome_executable(&chrome_path)
            .headless_mode(headless_mode)
            .window_size(1280, 800)
            // Required in any container environment:
            .arg("--no-sandbox") // no setuid sandbox in containers
            .arg("--disable-gpu") // không có GPU
            .arg("--disable-dev-shm-usage") // /dev/shm limited trong container
            // Demo: Chrome window ở góc trên trái, không overlap terminal
            .arg("--window-position=0,0")
            // Optional nhưng nên có:
            .arg("--disable-software-rasterizer")
            .arg("--disable-extensions")
            .build()
            .map_err(|e| anyhow::anyhow!(e))?;

        let (browser, mut handler) = Browser::launch(config).await?;

        // Spawn handler loop — chromiumoxide yêu cầu
        tokio::spawn(async move { while let Some(_) = handler.next().await {} });

        let page = browser.new_page(starting_url).await?;

        Ok(Self {
            page: Arc::new(Mutex::new(page)),
            _browser: browser,
        })
    }

    pub async fn screenshot(&self) -> anyhow::Result<Vec<u8>> {
        let page = self.page.lock().await;
        Ok(page
            .screenshot(
                chromiumoxide::page::ScreenshotParams::builder()
                    .format(
                        chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat::Png,
                    )
                    .build(),
            )
            .await?)
    }

    pub async fn inspect_target_snapshot(
        &self,
        target: &ActionTarget,
    ) -> anyhow::Result<Option<ElementSnapshot>> {
        let page = self.page.lock().await;
        let Some(el) = self.find_for_inspection(&page, target).await? else {
            return Ok(None);
        };

        let js_fn = r#"
            function() {
                return {
                    tag: this.tagName,
                    type: this.getAttribute('type'),
                    name: this.getAttribute('name'),
                    id: this.id || null,
                    autocomplete: this.getAttribute('autocomplete'),
                    aria_label: this.getAttribute('aria-label'),
                    data_testid: this.getAttribute('data-testid'),
                    text: (this.innerText || '').slice(0, 120),
                    inputmode: this.getAttribute('inputmode')
                };
            }
        "#;

        let result = el.call_js_fn(js_fn, false).await?;
        let Some(value) = result.result.value else {
            return Ok(None);
        };

        let snapshot = match value {
            Value::Object(_) => serde_json::from_value::<ElementSnapshot>(value)
                .map_err(|e| anyhow::anyhow!("snapshot parse error: {}", e))?,
            _ => {
                warn!("snapshot parse skipped: non-object value");
                return Ok(None);
            }
        };

        Ok(Some(snapshot))
    }

    /// Execute action — Done và Escalate KHÔNG được gọi hàm này (checked upstream)
    pub async fn execute(&self, action: &AgentAction) -> anyhow::Result<String> {
        let page = self.page.lock().await;

        match action {
            AgentAction::Click { target } => {
                self.find_resilient(&page, target).await?.click().await?;
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                Ok("Clicked element".to_string())
            }
            AgentAction::Type { target, value } => {
                let el = self.find_resilient(&page, target).await?;
                el.click().await?;
                el.type_str(value).await?;
                Ok(format!("Typed: {}", value))
            }
            AgentAction::Navigate { url } => {
                page.goto(url.as_str()).await?;
                Ok(format!("Navigated to {}", url))
            }
            AgentAction::Scroll { direction } => {
                let script = if direction == "down" {
                    "window.scrollBy(0, 400)"
                } else {
                    "window.scrollBy(0, -400)"
                };
                page.evaluate(script).await?;
                Ok(format!("Scrolled {}", direction))
            }
            AgentAction::Wait { reason } => {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                Ok(format!("Waited: {}", reason))
            }
            // Terminal/Dialogue states — không bao giờ reach đây (ADR-015/Dialogue)
            AgentAction::Done { .. }
            | AgentAction::Escalate { .. }
            | AgentAction::AskUser { .. } => {
                unreachable!(
                    "Terminal/Dialogue states handled before execute() is called — ADR-015"
                )
            }
        }
    }

    /// Fallback chain: CSS → ARIA label → text content → coordinates (ADR-015)
    async fn find_resilient(
        &self,
        page: &Page,
        target: &ActionTarget,
    ) -> anyhow::Result<chromiumoxide::element::Element> {
        // 1. CSS selector (stable ids, data-*, aria attrs preferred)
        if let Some(css) = &target.css {
            if let Ok(el) = page.find_element(css.as_str()).await {
                return Ok(el);
            }
        }

        // 2. ARIA label
        if let Some(label) = &target.aria_label {
            let sel = format!("[aria-label='{}']", label);
            if let Ok(el) = page.find_element(sel.as_str()).await {
                return Ok(el);
            }
        }

        // 3. Visible text content (XPath)
        if let Some(text) = &target.text_content {
            let xpath = format!(
                "xpath///button[contains(text(),'{}')]|//a[contains(text(),'{}')]|//span[contains(text(),'{}')]",
                text, text, text
            );
            if let Ok(el) = page.find_element(xpath.as_str()).await {
                return Ok(el);
            }
        }

        // 4. Coordinate click — last resort
        if let Some((x, y)) = target.coordinates {
            page.evaluate(format!("document.elementFromPoint({},{})?.click()", x, y))
                .await?;
            // Body là dummy return — coordinate click không cần element ref
            return page
                .find_element("body")
                .await
                .map_err(|e| anyhow::anyhow!("Coordinate click fallback failed: {}", e));
        }

        Err(anyhow::anyhow!(
            "Cannot find element — tried CSS={:?}, ARIA={:?}, text={:?}, coords={:?}",
            target.css,
            target.aria_label,
            target.text_content,
            target.coordinates
        ))
    }

    async fn find_for_inspection(
        &self,
        page: &Page,
        target: &ActionTarget,
    ) -> anyhow::Result<Option<Element>> {
        if let Some(css) = &target.css {
            if let Ok(el) = page.find_element(css.as_str()).await {
                return Ok(Some(el));
            }
        }

        if let Some(label) = &target.aria_label {
            let sel = format!("[aria-label='{}']", label);
            if let Ok(el) = page.find_element(sel.as_str()).await {
                return Ok(Some(el));
            }
        }

        if let Some(text) = &target.text_content {
            let xpath = format!(
                "xpath///button[contains(text(),'{}')]|//a[contains(text(),'{}')]|//span[contains(text(),'{}')]",
                text, text, text
            );
            if let Ok(el) = page.find_element(xpath.as_str()).await {
                return Ok(Some(el));
            }
        }

        Ok(None)
    }

    /// ADR-031: Extract interactive DOM elements for hybrid navigation.
    /// Returns a compact text summary of clickable/typeable elements,
    /// allowing the reasoning model to prefer DOM selectors over coordinates.
    pub async fn extract_dom_context(&self) -> anyhow::Result<String> {
        let page = self.page.lock().await;

        let js = r#"
            (function() {
                const els = document.querySelectorAll(
                    'a[href], button, input, select, textarea, [role="button"], [onclick]'
                );
                const items = [];
                const seen = new Set();
                for (const el of els) {
                    if (items.length >= 30) break;
                    const tag = el.tagName.toLowerCase();
                    const label = el.getAttribute('aria-label') || el.innerText?.trim().slice(0, 40) || '';
                    const css = el.id ? '#' + el.id
                        : el.getAttribute('data-testid') ? `[data-testid="${el.getAttribute('data-testid')}"]`
                        : el.getAttribute('aria-label') ? `[aria-label="${el.getAttribute('aria-label')}"]`
                        : null;
                    const key = tag + ':' + label.slice(0, 20);
                    if (seen.has(key) || !label) continue;
                    seen.add(key);
                    let desc = `<${tag}`;
                    if (css) desc += ` css="${css}"`;
                    if (el.type) desc += ` type="${el.type}"`;
                    if (label) desc += ` label="${label.slice(0, 40)}"`;
                    if (el.href) desc += ` href="${el.href.slice(0, 60)}"`;
                    desc += '>';
                    items.push(desc);
                }
                return items.join('\n');
            })()
        "#;

        let result = page.evaluate(js).await?;
        let context = result.into_value::<String>().unwrap_or_default();

        if context.is_empty() {
            return Err(anyhow::anyhow!("No interactive elements found on page"));
        }

        Ok(context)
    }
}
