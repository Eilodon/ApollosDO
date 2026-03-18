# CONTRACTS.md — Schema Registry
### Apollos UI Navigator · v0.3.2

> **Nguyên tắc vàng:** Mọi type, schema, enum, constant được define **MỘT LẦN DUY NHẤT** tại đây.
> BLUEPRINT.md và code **reference** — không redefine, không copy, không paraphrase.
>
> Khi thấy conflict giữa file này và bất kỳ file nào khác → file này thắng.

---

## Mục lục

1. [Primitive Types & Constants](#1-primitive-types--constants)
2. [Enums](#2-enums)
3. [Core Schemas](#3-core-schemas)
4. [Input / Output Contracts](#4-input--output-contracts)
5. [Error Registry](#5-error-registry)
6. [External Contracts](#6-external-contracts)
7. [Naming Conventions](#7-naming-conventions)
8. [Schema Changelog](#8-schema-changelog)

---

## 1. PRIMITIVE TYPES & CONSTANTS

> Các kiểu và hằng số dùng xuyên suốt hệ thống.
> Agent KHÔNG hard-code giá trị của các constants này ở bất kỳ nơi nào khác.

```
DEMO_SESSION_ID :: string = "demo-session-001"
  // Lý do: Fixed session ID cho demo mode

MAX_STEPS :: u32 = 20
  // Lý do: Giới hạn số bước để tránh infinite loop trong agent execution

MAX_STABLE_WAIT :: u32 = 5
  // Lý do: Sau 5 frames stable -> force Gradient call để tránh deadlock

NOVA_MIN_GAP_S :: f64 = 1.0
  // Lý do: Tối thiểu 1.0s giữa các Gradient calls (tăng từ 0.8s để tránh 429 trên DO)
  // ADR-014: Gradient rate limits thấp hơn Gemini → cần gap lớn hơn

NOVA_BURST_LIMIT :: usize = 4
  // Lý do: Tối đa 4 calls trong burst window (giảm từ 6 để an toàn với DO quota)
  // ADR-014

NOVA_BURST_WINDOW_S :: f64 = 15.0
  // Lý do: Burst window 15s cho rate limiting

NOVA_BACKOFF_MS :: u64 = 1000
  // Lý do: Backoff 1000ms khi Gradient bị rate limit (tăng từ 800ms)
  // ADR-014

USER_REPLY_TIMEOUT_S :: u64 = 120
  // Lý do: Timeout 120s cho user reply

SESSION_ID_LENGTH :: usize = 36
  // Lý do: UUID v4 length cho session ID

// Keyword arrays cho sensitive content detection
PAYMENT_KEYWORDS :: List<string> = [
  "thanh toán", "thanh toan", "payment", "pay", "checkout",
  "mua ngay", "dat hang", "đặt hàng", "chuyen khoan",
  "chuyển khoản", "bank", "ngan hang", "ngân hàng",
  "wallet", "ví", "nap tien"
]

OTP_KEYWORDS :: List<string> = [
  "otp", "one-time", "ma otp", "mã otp", "ma xac nhan",
  "mã xác nhận", "verification code", "2fa", "two-factor",
  "ma 2fa", "mã 2fa", "verify"
]

PASSWORD_KEYWORDS :: List<string> = [
  "password", "mat khau", "mật khẩu", "passcode", "pin",
  "doi mat khau", "đổi mật khẩu", "reset password",
  "new password", "current password"
]

ACCOUNT_KEYWORDS :: List<string> = [
  "account", "tai khoan", "tài khoản", "dang nhap",
  "đăng nhập", "login", "sign in", "sign-in",
  "change email", "doi email", "change phone", "xoa tai khoan"
]


STUCK_THRESHOLD :: usize = 3
  // Lý do: Nếu cùng action lặp lại 3 lần → agent stuck → NeedHuman (ADR-026)

SEMANTIC_DIFF_THRESHOLD :: f64 = 0.05
  // Lý do: Pixel change < 5% = "semantically same page" → skip Gradient call (ADR-024)

SSE_REPLAY_BUFFER_SIZE :: usize = 50
  // Lý do: Max messages kept for late SSE subscribers replay (ADR-030)

DIALOGUE_HISTORY_MAX :: usize = 10
  // Lý do: Max AskUser turns giữ trong dialogue_history (ADR-029)

STEP_HISTORY_WINDOW :: usize = 5
  // Lý do: Số steps gần nhất inject vào context (tăng từ 3 lên 5 — ADR-029)

ASK_USER_MAX_TURNS :: usize = 3
  // Lý do: Tối đa 3 lần hỏi user trong 1 task (enforce ADR-016 guideline)

// URL Validation patterns (ADR-027)
BLOCKED_URL_PROTOCOLS :: List<string> = ["javascript:", "data:", "file:", "vbscript:"]
  // Lý do: Các protocol nguy hiểm không bao giờ được navigate

PAYMENT_URL_PATTERNS :: List<string> = ["checkout", "payment", "pay/", "/pay?", "billing", "purchase"]
  // Lý do: URL patterns trigger escalation thay vì navigate

LOCAL_IP_PREFIXES :: List<string> = ["192.168.", "10.0.", "172.16.", "127.0.0.1", "localhost"]
  // Lý do: Block local network access từ agent

// --- Environment & AI Config ---

GRADIENT_API_KEY :: string
  // Lý do: API key cho DigitalOcean Gradient™ AI inference
  // Lấy tại: https://cloud.digitalocean.com/gen-ai
  // ADR-012

GRADIENT_ENDPOINT :: string = "https://inference.do-ai.run/v1/chat/completions"
  // Lý do: DO Gradient inference endpoint (OpenAI-compatible)
  // ADR-012

BROWSER_AGENT_MODEL :: string = "llama3.2-vision"
  // Lý do: Model mặc định cho reasoning agent trên DO Gradient
  // ADR-012: Thay thế gemini-2.0-flash

// --- Browser Config ---

BROWSER_HEADLESS :: boolean = true
  // Lý do: Chế độ chạy trình duyệt (ẩn hoặc hiện)

CHROME_EXECUTABLE :: string? = null
  // Lý do: Đường dẫn cụ thể đến Chrome (nếu có, auto-detect nếu null)

SAFE_MODE_MAX_LOOPS :: u32 = 10
  // Lý do: Tối đa 10 × 30s = 5 phút cho activate_safe_mode() — prevent infinite loop (ADR-032)
  // Sau max loops: trả về DigitalResult::NeedHuman regardless
```

> **Type notation dùng trong file này:**
> ```
> FieldName :: Type                       — required field
> FieldName :: Type?                      — optional field (nullable)
> FieldName :: List<Type>                 — list
> FieldName :: Map<KeyType, ValueType>    — map / dict
> FieldName :: TypeA | TypeB             — union type
> FieldName :: Ref<SchemaName>            — reference đến schema khác
> ```

---

## 2. ENUMS

> Mọi enum được define tại đây. Không tạo inline enum trong schema.

### MotionState

```
MotionState ::
  | Stationary     // User không di chuyển — an toàn để thực hiện digital task
  | WalkingSlow    // User đi bộ chậm — có thể thực hiện nhưng cần cảnh báo
  | WalkingFast    // User đi bộ nhanh — không nên thực hiện digital task
  | Running        // User đang chạy — không cho phép digital task
  | Unspecified    // Trạng thái không xác định — mặc định là Stationary
```

**Dùng ở:** `DigitalSessionContext`, `SessionStore`
**Không dùng cho:** Motion detection hardware layer


### NavigateDecision

> Kết quả validate URL trước khi execute AgentAction::Navigate (ADR-027)

```
NavigateDecision ::
  | Allow                   // URL safe — proceed với navigation
  | Escalate(reason: string) // URL suspicious — trigger human escalation
  | Block(reason: string)   // URL dangerous — reject, return Failed
```

**Dùng ở:** `DigitalAgent` (validate trước execute Navigate)
**Không dùng cho:** Click/Type targets (handled by sensitive_guard)

### AgentAction

```
AgentAction ::
  | Click     { target: Ref<ActionTarget> }                   // Nhấn vào element
  | Type      { target: Ref<ActionTarget>, value: string }    // Gõ text vào input
  | Navigate  { url: string }                                  // Mở URL
  | Scroll    { direction: string }                           // Cuộn trang (up/down)
  | Wait      { reason: string }                              // Chờ với lý do
  | Done      { summary: string }                             // Hoàn thành task
  | Escalate  { reason: string }                              // Chuyển cho human
  | AskUser   { question: string }                            // Hỏi user trước khi hành động
```

**Dùng ở:** `NovaReasoningClient`, `BrowserExecutor`, `DigitalAgent`
**Note:** `AskUser` là action quan trọng cho safety — agent PHẢI dùng khi intent mơ hồ
trước khi bắt đầu browser navigation (ADR-016).

### BackendToClientMessage

```
BackendToClientMessage ::
  | AssistantText { message: Ref<AssistantTextMessage> }       // Text message từ assistant
  | HumanHelpSession { message: Ref<HumanHelpSessionMessage> } // Human handoff session
```

**Dùng ở:** `WebSocketRegistry`

### DigitalResult

```
DigitalResult ::
  | Done(string)        // Task hoàn thành với summary
  | NeedHuman(string)   // Cần human intervention
  | Failed(string)      // Task thất bại với error message
```

**Dùng ở:** `DigitalAgent` return type

---

## 3. CORE SCHEMAS

> Schemas được sắp xếp từ primitive → composite.
> Schema phụ thuộc schema khác → schema kia phải được define TRÊN nó.

---

### ActionTarget

> Target cho browser action — định nghĩa element cần tương tác

```
ActionTarget :: {
  css                   :: string?                    // CSS selector để tìm element
  aria_label            :: string?                    // aria-label attribute
  text_content          :: string?                    // Text content của element
  coordinates           :: (f64, f64)?               // (x, y) coordinates cho click
}
```

**Constraints:**
```
INVARIANT: Ít nhất một trong các field phải present
INVARIANT: coordinates chỉ dùng khi css, aria_label và text_content đều không tìm được
FALLBACK ORDER: css → aria_label → text_content → coordinates (ADR-001)
```

---

### ElementSnapshot

> Snapshot của DOM element từ browser inspection

```
ElementSnapshot :: {
  tag                   :: string?                    // HTML tag name
  type_attr             :: string?                    // type attribute (input, button, etc.)
  name                  :: string?                    // name attribute
  id                    :: string?                    // id attribute
  autocomplete          :: string?                    // autocomplete attribute
  aria_label            :: string?                    // aria-label attribute
  data_testid           :: string?                    // data-testid attribute
  text                  :: string?                    // Visible text content (max 120 chars)
  inputmode             :: string?                    // inputmode attribute
}
```

**Constraints:**
```
INVARIANT: text và aria_label không được null cùng lúc
RANGE: text.length ≤ 120 — truncated at browser inspection
```

---

### AssistantTextMessage

> Message typed từ assistant đến user qua live transport typed payload (WebSocket path)

```
AssistantTextMessage :: {
  session_id            :: string                     // UUID v4
  timestamp_ms          :: u64                        // Unix timestamp in milliseconds
  text                  :: string                     // Message content
}
```

**Constraints:**
```
INVARIANT: session_id phải là valid UUID v4
INVARIANT: text không được rỗng
```

---

### HumanHelpSessionMessage

> Message khi chuyển cho human assistance

```
HumanHelpSessionMessage :: {
  session_id            :: string                     // UUID v4
  timestamp_ms          :: u64                        // Unix timestamp in milliseconds
  help_link             :: string?                    // Link để human help (Twilio, etc.)
}
```

---

### SessionState

> Internal session state stored in SessionStore

```
SessionState :: {
  session_id                  :: string                     // UUID v4
  created_at                  :: timestamp                  // Creation time
  last_seen                   :: timestamp                  // Last activity
  motion_state                :: Ref<MotionState>           // User motion state
  digital_agent_handle        :: Ref<DigitalAgentHandle>?   // Running agent instance
  browser_executor            :: Ref<Arc<Mutex<Option<Arc<BrowserExecutor>>>>>  // Browser slot
  reply_tx_slot               :: Ref<Arc<Mutex<Option<UserReplyTx>>>>           // Reply channel slot
  nova_call_timestamps        :: List<f64>                 // Recent Gradient call times
  nova_call_total             :: u64                        // Total Gradient calls
  dialogue_history            :: List<string>              // AskUser Q&A pairs — NEVER truncated (ADR-029)
  step_history                :: List<string>              // Recent action steps — truncated to STEP_HISTORY_WINDOW (ADR-029)
  action_key_history          :: List<string>              // Recent action keys for stuck detection (ADR-026)
  ask_user_count              :: u32                       // Number of AskUser turns this session
}
```

**Constraints:**
```
INVARIANT: session_id phải là valid UUID v4
INVARIANT: nova_call_timestamps chỉ giữ calls trong 1 giờ gần nhất
INVARIANT: browser_executor slot phải được clear về None khi cancel hoặc task kết thúc (ADR-017)
```

---

### ConversationTurn

> Một lượt trao đổi trong conversation

```
ConversationTurn :: {
  question              :: string                     // Câu hỏi từ agent
  answer                :: string                     // Câu trả lời từ user
}
```

**Constraints:**
```
INVARIANT: question và answer không được rỗng
```

---

### DigitalAgentHandle

> Handle để quản lý running DigitalAgent instance

```
DigitalAgentHandle :: {
  cancel               :: Ref<CancellationToken>          // Cancellation token
  task                 :: Ref<JoinHandle<DigitalResult>>  // Async task handle
}
```

---

### DigitalSessionContext

> Context cho digital agent execution

```
DigitalSessionContext :: {
  motion_state          :: Ref<MotionState>                          // User motion state
  session_id            :: string                                    // UUID v4
  ws_registry           :: Ref<WebSocketRegistry>                    // Optional live WebSocket broadcast
  fallback              :: Ref<HumanFallbackService>                 // Human escalation service
  sessions              :: Ref<SessionStore>                         // Session persistence
  reply_tx_slot         :: Arc<Mutex<Option<UserReplyTx>>>           // Channel cho user reply
  browser_executor_slot :: Arc<Mutex<Option<Arc<BrowserExecutor>>>>  // Browser instance slot
}
```

---

### UserReplyTx / UserReplyRx

> Channel types cho user reply

```
UserReplyTx :: oneshot::Sender<string>
UserReplyRx :: oneshot::Receiver<string>
```

---

## 4. INPUT / OUTPUT CONTRACTS

> I/O contract của từng entry point / API boundary trong hệ thống.
> Đây là "giao kèo" giữa các components — không thay đổi mà không có ADR entry.

---

### execute_with_cancel()

> Main entry point cho digital agent execution

```
INPUT  :: intent: string, cancel: Ref<CancellationToken>, ctx: Ref<DigitalSessionContext>

OUTPUT :: Ref<DigitalResult>

SIDE EFFECTS:
  - Browser automation  : Mở Chrome, navigate, click, type
  - Status bus publish  : Gửi demo status strings vào shared replay-backed stream
  - WebSocket broadcast : Gửi AssistantText messages nếu có live socket registered
  - Session state       : Cập nhật nova call timestamps
  - External calls      : DO Gradient AI inference API

PRE-CONDITIONS:
  - intent phải không rỗng
  - ctx.session_id phải là valid UUID
  - Browser phải khả dụng
  - GRADIENT_API_KEY phải configured

POST-CONDITIONS:
  - browser_executor_slot được clear về None (ADR-017)
  - Session state được cập nhật
  - Demo status stream đã nhận được narration/question/final status

IDEMPOTENT: KHÔNG — vì có side effects (browser actions, API calls)
```

---

### next_action() / next_action_with_cancel()

> DO Gradient AI vision reasoning call

```
INPUT  :: screenshot: Vec<u8>, intent: string, dialogue_history: List<string>, step_history: List<string>, step: u32
          (next_action_with_cancel nhận thêm: cancel: Option<Ref<CancellationToken>>)

OUTPUT :: Ref<AgentAction>
       | Ref<ERR_GRADIENT_AUTH>   // khi GRADIENT_API_KEY sai
       | Ref<ERR_RATE_LIMITED>    // khi 429 sau 3 retries
       | Ref<ERR_NOVA_REASONING>  // khi 5xx hoặc parse error

SIDE EFFECTS:
  - External API call : DO Gradient inference endpoint
  - Retry logic       : Exponential backoff trên 429 (ADR-014)

PRE-CONDITIONS:
  - screenshot phải là valid PNG bytes
  - step phải trong [1, MAX_STEPS]
  - GRADIENT_API_KEY phải valid

POST-CONDITIONS:
  - Trả về AgentAction parsed từ JSON response của Llama 3.2 Vision

IDEMPOTENT: CÓ — same input produces same action (deterministic với temperature=0.1)
```

---

### demo/start_task

> Demo endpoint để bắt đầu task

```
INPUT  :: {
  intent: string,
  motion_state?: string  // "stationary" | "walking_slow" | "walking_fast" | "running" (ADR-035)
                         // Optional — defaults to Stationary nếu không có
}

OUTPUT ::
  { task_id: string, status: "started" }           // Normal digital task
  | { task_id: string, status: "physical_safety_mode",  // ADR-035: motion gate triggered
      message: string, intent: string, motion_state: string? }
  | Ref<ERR_DEMO_MODE>

SIDE EFFECTS:
  - clear_replay_buffer()              : Xóa replay buffer (ADR-033) — FIRST action
  - classify_intent() check            : Physical → halt, không spawn agent (ADR-035)
  - Session creation : touch_session() hoặc reuse existing
  - Agent spawn      : Start DigitalAgent trong background tokio task (nếu Digital intent)
  - Cancel previous  : cancel_digital_agent() nếu có task đang chạy

PRE-CONDITIONS:
  - DEMO_MODE=1
  - intent không rỗng

POST-CONDITIONS:
  - Nếu Physical: replay buffer đã clear, không có agent spawn
  - Nếu Digital: DigitalAgentHandle được lưu vào SessionStore, agent bắt đầu execution

IDEMPOTENT: KHÔNG — cancel task cũ và tạo task mới
```

---

### demo/user_reply

> Demo endpoint để trả lời agent question

```
INPUT  :: { answer: string }

OUTPUT :: { status: "delivered", answer: string }
        | { status: "no_pending_question", note: string }

SIDE EFFECTS:
  - Channel send : Gửi answer vào waiting DigitalAgent qua oneshot channel

PRE-CONDITIONS:
  - DEMO_MODE=1
  - answer không rỗng

POST-CONDITIONS:
  - Nếu delivered: DigitalAgent nhận được answer và tiếp tục execution
  - reply_tx_slot được clear về None sau khi send

IDEMPOTENT: KHÔNG — timing matters, oneshot channel consumed sau khi send
```

---

### demo/status

> Demo SSE endpoint để stream status realtime có replay buffer

```
INPUT  :: none

OUTPUT :: stream<string>

SIDE EFFECTS:
  - Replay snapshot : Subscriber mới nhận buffered status messages trước
  - Live subscribe  : Subscriber tiếp tục nhận status messages mới qua broadcast channel

PRE-CONDITIONS:
  - DEMO_MODE=1

POST-CONDITIONS:
  - Không mutate session state

IDEMPOTENT: CÓ — read-only stream endpoint
```

---

### demo

> Demo web page dùng browser-native speech recognition và speech synthesis

```
INPUT  :: none

OUTPUT :: text/html

SIDE EFFECTS:
  - Browser EventSource : Connect tới `/demo/status`
  - Browser POST        : Gửi request tới `/demo/start_task`, `/demo/user_reply`, `/demo/trigger_hard_stop`
  - Browser speech APIs : SpeechRecognition/webkitSpeechRecognition và speechSynthesis (demo path only)

PRE-CONDITIONS:
  - DEMO_MODE=1
  - Browser nên hỗ trợ Web Speech API để dùng voice path

POST-CONDITIONS:
  - Không mutate server state cho tới khi user submit intent/reply

IDEMPOTENT: CÓ — serving static demo shell
```

---

### cancel_digital_agent()

> Cancel running agent với cleanup đầy đủ

```
INPUT  :: session_id: string, reason: DigitalAgentCancelReason

OUTPUT :: unit

SIDE EFFECTS:
  - CancellationToken.cancel()             : Signal agent dừng tại await point tiếp theo
  - browser_executor_slot ← None           : Clear browser slot để Chrome process có thể drop (ADR-017)
  - digital_agent_handle ← None            : Remove handle sau khi cancel
  - Cancel metrics increment               : Theo reason

PRE-CONDITIONS:
  - session_id phải tồn tại trong SessionStore

POST-CONDITIONS:
  - Chrome process sẽ drop khi Arc refcount về 0
  - Agent sẽ return DigitalResult::Failed trong vòng < 1s (ADR-005)

IDEMPOTENT: CÓ — cancel trên token đã cancel là no-op
```

---

### should_allow_nova_call()

> Check if Gradient API call is allowed based on rate limiting

```
INPUT  :: session_id: string, now: f64, min_gap_s: f64, burst_limit: usize, burst_window_s: f64

OUTPUT :: bool

SIDE EFFECTS:
  - Nếu allowed: push current timestamp vào nova_call_timestamps
  - Nếu blocked: record_nova_blocked() được gọi by caller

PRE-CONDITIONS:
  - session_id phải là valid UUID

POST-CONDITIONS:
  - Returns true nếu: (now - last_call) ≥ min_gap_s VÀ recent_calls_in_window < burst_limit

IDEMPOTENT: KHÔNG — side effect khi allowed (push timestamp)
```

---

## 5. ERROR REGISTRY

> Mọi error code được define tại đây với HTTP status tương ứng (nếu có),
> message template, và context cần thiết để debug.

| Code | HTTP | Message Template | Context cần thiết | Khi nào xảy ra |
|---|---|---|---|---|
| `ERR_BROWSER_INIT` | 500 | `"Không thể khởi động browser: {error}"` | `error` | Chrome không available hoặc crash |
| `ERR_SCREENSHOT` | 500 | `"Screenshot lỗi: {error}"` | `error` | Browser CDP error |
| `ERR_NOVA_REASONING` | 500 | `"Gradient reasoning lỗi: {error}"` | `error` | DO Gradient API 5xx hoặc parse error |
| `ERR_GRADIENT_AUTH` | 401 | `"GRADIENT_AUTH_FAIL: API key không hợp lệ"` | none | GRADIENT_API_KEY sai hoặc hết hạn |
| `ERR_BROWSER_EXECUTE` | 500 | `"Browser execute lỗi: {error}"` | `error` | Element not found hoặc CDP error |
| `ERR_DEMO_MODE` | 403 | `"Demo mode không enabled"` | none | Gọi demo endpoint khi DEMO_MODE=0 |
| `ERR_NO_PENDING_QUESTION` | 400 | `"Không có câu hỏi đang chờ trả lời"` | none | User reply khi agent không hỏi |
| `ERR_SESSION_NOT_FOUND` | 404 | `"Session không tồn tại: {session_id}"` | `session_id` | Invalid session ID |
| `ERR_RATE_LIMITED` | 429 | `"Gradient rate limited — đã retry 3 lần"` | `attempt` | DO Gradient 429 sau max retries (ADR-014) |
| `ERR_CANCELLED` | 499 | `"Bị gián đoạn: {reason}"` | `reason` | CancellationToken triggered |

> **Error format chuẩn:**
> ```
> Error :: {
>   code    :: ErrorCode        // từ registry này
>   message :: string           // theo message template
>   context :: Map<string, any> // các fields liệt kê trong cột "Context"
>   trace   :: string?          // optional, chỉ trong dev mode
> }
> ```

> **Đã xóa:** `ERR_SDK_BRIDGE` — Python bridge đã được remove (ADR-013).
> Error code này không còn applicable.

---

## 6. EXTERNAL CONTRACTS

> Interface với các external services, third-party APIs, hoặc databases.
> Ghi lại những gì hệ thống này *expect* từ bên ngoài — không phải implementation của bên ngoài.

### DigitalOcean Gradient™ AI (Llama 3.2 Vision)

```
// Hệ thống này gọi DO Gradient với:
REQUEST :: {
  model: "llama3.2-vision",
  messages: [
    {
      role: "system",
      content: string    // Short system prompt < 400 chars (ADR-016)
    },
    {
      role: "user",
      content: [
        {
          type: "image_url",
          image_url: { url: "data:image/png;base64,{base64_screenshot}" }
        },
        {
          type: "text",
          text: string   // Intent + recent history (last 3 steps) + step number
        }
      ]
    }
  ],
  max_tokens: 256,
  temperature: 0.1
}

// Auth:
HEADER :: Authorization: Bearer {GRADIENT_API_KEY}

// Hệ thống này expect Gradient trả về:
RESPONSE :: {
  choices: [{
    message: {
      content: string    // JSON của AgentAction (có thể có markdown fences)
    }
  }]
}

// Failure modes hệ thống phải handle:
FAILURES ::
  | 429           // Rate limit → exponential backoff, max 3 retries (ADR-014)
                  // Backoff: 2s, 4s, 8s
  | 401           // Auth fail → ERR_GRADIENT_AUTH, fail fast, no retry
  | 503 (first)   // Unavailable → 1 retry sau 2s
  | 503 (second)  // Unavailable again → ERR_NOVA_REASONING
  | PARSE_ERROR   // JSON parse fail → ERR_NOVA_REASONING
  | TIMEOUT       // sau 30s → ERR_NOVA_REASONING
```

### Chrome DevTools Protocol (CDP)

```
// Hệ thống này gọi CDP với:
REQUEST :: {
  method: "Runtime.evaluate",
  params: {
    expression: string,    // JavaScript để inspect element
    returnByValue: true
  }
}

// Hệ thống này expect CDP trả về:
RESPONSE :: {
  result: {
    value: Ref<ElementSnapshot>
  }
}

// Failure modes hệ thống phải handle:
FAILURES ::
  | TARGET_CLOSED     // Chrome crashed → ERR_BROWSER_EXECUTE
  | NO_TARGET         // Element not found → ERR_BROWSER_EXECUTE
  | JS_EXCEPTION      // JavaScript error → ERR_BROWSER_EXECUTE
```

---

## 7. NAMING CONVENTIONS

> Quy ước đặt tên xuyên suốt codebase. Agent phải tuân theo khi generate code.

| Context | Convention | Ví dụ |
|---|---|---|
| Schema names | `PascalCase` | `DigitalSessionContext`, `ActionTarget` |
| Field names | `snake_case` | `session_id`, `timestamp_ms` |
| Constants | `SCREAMING_SNAKE` | `MAX_STEPS`, `NOVA_MIN_GAP_S` |
| Functions | `snake_case` | `execute_with_cancel()`, `next_action()` |
| Enum variants | `PascalCase` | `Stationary`, `WalkingSlow` |
| Error codes | `SCREAMING_SNAKE` | `ERR_BROWSER_INIT`, `ERR_GRADIENT_AUTH` |
| Module names | `snake_case` | `digital_agent`, `browser_executor` |

**Domain-specific rules:**

```
Session IDs: UUID v4 format (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)
  ✅ "550e8400-e29b-41d4-a716-446655440000"
  ❌ "session_123", "abc123"

Timestamps: Always milliseconds since Unix epoch
  ✅ 1704067200000
  ❌ 1704067200 (seconds), "2024-01-01T00:00:00Z"

Motion States: Vietnamese lowercase with underscores
  ✅ "walking_slow", "stationary"
  ❌ "WALKING_SLOW", "slow_walk"

Agent Actions: Verb-first, descriptive
  ✅ "click_button", "navigate_to_url", "type_in_search"
  ❌ "button_click", "url_navigate"
```

---

## 8. SCHEMA CHANGELOG

> Append-only. Mọi thay đổi schema đều phải có entry ở đây.
> Breaking changes phải có ADR entry tương ứng trong ADR.md.

| Version | Date | Schema | Thay đổi | Breaking? | ADR Ref |
|---|---|---|---|---|---|
| v0.1.0 | 2024-01-01 | — | Init schema registry | — | — |
| v0.1.1 | 2024-03-15 | — | Đồng bộ hóa với Gemini & SDK Bridge | Có | ADR-010, ADR-011 |
| v0.2.0 | 2026-03-18 | Constants | GEMINI_API_KEY → GRADIENT_API_KEY, BROWSER_AGENT_MODEL → llama3.2-vision, NOVA_MIN_GAP_S: 0.8→1.0, NOVA_BURST_LIMIT: 6→4, NOVA_BACKOFF_MS: 800→1000 | Có | ADR-012 |
| v0.2.0 | 2026-03-18 | SessionState | browser_executor type: `Option<BrowserExecutor>` → `Option<Arc<BrowserExecutor>>` | Có | ADR-017 |
| v0.2.0 | 2026-03-18 | Error Registry | REMOVED: ERR_SDK_BRIDGE; ADDED: ERR_GRADIENT_AUTH | Có | ADR-013 |
| v0.2.0 | 2026-03-18 | AgentAction | AskUser variant đã có nhưng documented rõ hơn role | Không | ADR-016 |
| v0.2.0 | 2026-03-18 | External Contracts | Gemini Vision API → DO Gradient AI | Có | ADR-012 || v0.3.0 | 2026-03-18 | Constants | ADDED: STUCK_THRESHOLD, SEMANTIC_DIFF_THRESHOLD, SSE_REPLAY_BUFFER_SIZE, DIALOGUE_HISTORY_MAX, STEP_HISTORY_WINDOW, ASK_USER_MAX_TURNS, URL validation constants | Không | ADR-026,027,029,030 |
| v0.3.0 | 2026-03-18 | Enums | ADDED: NavigateDecision | Không | ADR-027 |
| v0.3.0 | 2026-03-18 | SessionState | ADDED: dialogue_history, step_history, action_key_history, ask_user_count | Có | ADR-029, ADR-026 |
| v0.3.0 | 2026-03-18 | Error Registry | ADDED: ERR_STUCK_LOOP, ERR_NAVIGATE_BLOCKED | Không | ADR-026, ADR-027 |
| v0.3.1 | 2026-03-18 | Constants | ADDED: `SAFE_MODE_MAX_LOOPS=10` | Không | ADR-032 |
| v0.3.1 | 2026-03-18 | StartTaskRequest | ADDED: `motion_state: Option<string>` field | Không (backward-compat) | ADR-035 |
| v0.3.1 | 2026-03-18 | demo/start_task | OUTPUT added: `physical_safety_mode` response variant | Không | ADR-035 |
| v0.3.1 | 2026-03-18 | demo/start_task | SIDE EFFECTS added: `clear_replay_buffer()` first, `classify_intent()` check | Không | ADR-033, ADR-035 |
| v0.3.1 | 2026-03-18 | demo/user_reply | SIDE EFFECT changed: raw `status_tx.send()` → `broadcast_status()` | Không | ADR-034 |
| v0.3.1 | 2026-03-18 | Dockerfile | REMOVED: `python3-pip`, `google-generativeai` | Không | ADR-038 |
| v0.3.1 | 2026-03-18 | Files | ADDED: `.do/app.yaml`, `LICENSE` | Không | ADR-039, hackathon |
| v0.3.2 | 2026-03-18 | execute_with_cancel() | SIDE EFFECTS changed: shared `StatusBus` publish added before optional WebSocket live broadcast | Không | ADR-041 |
| v0.3.2 | 2026-03-18 | demo/status | ADDED: replay-backed SSE stream contract | Không | ADR-041 |
| v0.3.2 | 2026-03-18 | demo | ADDED: HTML demo shell contract using browser-native STT/TTS | Không | ADR-042 |
