package com.apollos.nativeapp

import android.content.Context
import android.util.Log
import java.io.FileInputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.channels.FileChannel
import kotlin.math.log10
import org.tensorflow.lite.DataType
import org.tensorflow.lite.Interpreter
import org.tensorflow.lite.Tensor

internal class AudioVadGate(
    context: Context,
    private val sampleRate: Int,
) {
    private val silero = SileroVad.load(context)
    private var noiseFloor = 220.0
    private var lastSpeechAtMs = 0L
    private var lastSileroAtMs = 0L
    private val speechHoldMs = 380L
    private val sileroHoldMs = 260L
    private val minRms = 140.0
    private val minSnrDb = 8.0
    private val sileroThreshold = 0.55f

    fun shouldSend(pcm16: ByteArray, length: Int, nowMs: Long): Boolean {
        silero?.let { vad ->
            val prob = vad.processPcm16(pcm16, length, sampleRate)
            if (!prob.isNaN() && prob >= sileroThreshold) {
                lastSileroAtMs = nowMs
            }
            if (!prob.isNaN()) {
                return (nowMs - lastSileroAtMs) <= sileroHoldMs
            }
        }
        return rmsFallback(pcm16, length, nowMs)
    }

    private fun rmsFallback(pcm16: ByteArray, length: Int, nowMs: Long): Boolean {
        var sum = 0.0
        var i = 0
        while (i + 1 < length) {
            val sample = (pcm16[i].toInt() and 0xFF) or (pcm16[i + 1].toInt() shl 8)
            sum += sample * sample
            i += 2
        }
        val frames = (length / 2).coerceAtLeast(1)
        val rms = kotlin.math.sqrt(sum / frames)
        val adaptiveThreshold = maxOf(minRms, noiseFloor * 1.8)
        val snrDb = if (noiseFloor > 1.0) {
            20.0 * log10((rms / noiseFloor).coerceAtLeast(1e-6))
        } else {
            0.0
        }
        if (rms > adaptiveThreshold && snrDb >= minSnrDb) {
            lastSpeechAtMs = nowMs
        } else {
            noiseFloor = ((noiseFloor * 0.995) + (rms * 0.005)).coerceIn(60.0, 2000.0)
        }
        return (nowMs - lastSpeechAtMs) <= speechHoldMs
    }
}

internal class SileroVad private constructor(
    private val interpreter: Interpreter,
    private val audioInputIndex: Int,
    private val audioFrameSize: Int,
    private val audioInputType: DataType,
    private val stateInputIndex: Int?,
    private val stateOutputIndex: Int?,
    private val inputBuffers: Array<ByteBuffer>,
    private val inputObjects: Array<Any>,
    private val stateInputBuffer: ByteBuffer?,
    private val sampleRateIndex: Int?,
    private val outputProbIndex: Int,
    private val outputProbType: DataType,
    private val outputProbBuffer: ByteBuffer,
    private val stateOutputBuffer: ByteBuffer?,
) {
    private val ring = FloatArray(audioFrameSize)
    private var ringIndex = 0
    private var ringFilled = false

    companion object {
        private const val DEFAULT_ASSET = "models/silero_vad.tflite"

        fun load(context: Context, assetPath: String = DEFAULT_ASSET): SileroVad? {
            val mapped = loadModel(context, assetPath) ?: return null
            return try {
                val options = Interpreter.Options().setNumThreads(2)
                val interpreter = Interpreter(mapped, options)
                build(interpreter)
            } catch (e: Exception) {
                Log.w("SileroVad", "Failed to init Silero VAD: ${e.message}")
                null
            }
        }

        private fun loadModel(context: Context, assetPath: String): ByteBuffer? {
            return try {
                context.assets.openFd(assetPath).use { afd ->
                    FileInputStream(afd.fileDescriptor).use { input ->
                        val channel = input.channel
                        channel.map(FileChannel.MapMode.READ_ONLY, afd.startOffset, afd.declaredLength)
                    }
                }
            } catch (e: Exception) {
                Log.w("SileroVad", "Silero VAD model missing at $assetPath")
                null
            }
        }

        private fun build(interpreter: Interpreter): SileroVad? {
            val inputCount = interpreter.inputTensorCount
            if (inputCount <= 0) {
                return null
            }
            val inputTensors = (0 until inputCount).map { index -> interpreter.getInputTensor(index) }
            val audioIndex = inputTensors
                .withIndex()
                .maxByOrNull { it.value.elementCount() }?.index ?: return null
            val audioTensor = inputTensors[audioIndex]
            val audioFrameSize = audioTensor.elementCount()
            if (audioFrameSize <= 0) {
                return null
            }
            val sampleRateIndex = inputTensors
                .withIndex()
                .firstOrNull { (index, tensor) ->
                    index != audioIndex && tensor.elementCount() == 1 && tensor.dataType() != DataType.FLOAT32
                }?.index
            val stateIndex = inputTensors
                .withIndex()
                .firstOrNull { (index, tensor) ->
                    index != audioIndex &&
                        index != sampleRateIndex &&
                        tensor.dataType() == DataType.FLOAT32
                }?.index
            val outputCount = interpreter.outputTensorCount
            val outputTensors = (0 until outputCount).map { index -> interpreter.getOutputTensor(index) }
            val outputProbIndex = outputTensors
                .withIndex()
                .filter { it.value.dataType() == DataType.FLOAT32 }
                .minByOrNull { it.value.elementCount() }?.index ?: 0
            val outputProbTensor = outputTensors[outputProbIndex]
            val stateOutputIndex = stateIndex?.let { stateInput ->
                outputTensors.withIndex().firstOrNull { (_, tensor) ->
                    tensor.dataType() == DataType.FLOAT32 &&
                        tensor.shape().contentEquals(inputTensors[stateInput].shape())
                }?.index
            }
            val inputBuffers = Array(inputCount) { index -> inputTensors[index].allocateBuffer() }
            val inputObjects: Array<Any> = Array(inputCount) { index -> inputBuffers[index] }
            val stateInputBuffer = stateIndex?.let { idx -> inputBuffers[idx] }
            val outputProbBuffer = outputProbTensor.allocateBuffer()
            val stateOutputBuffer = stateOutputIndex?.let { idx -> outputTensors[idx].allocateBuffer() }
            return SileroVad(
                interpreter = interpreter,
                audioInputIndex = audioIndex,
                audioFrameSize = audioFrameSize,
                audioInputType = audioTensor.dataType(),
                stateInputIndex = stateIndex,
                stateOutputIndex = stateOutputIndex,
                inputBuffers = inputBuffers,
                inputObjects = inputObjects,
                stateInputBuffer = stateInputBuffer,
                sampleRateIndex = sampleRateIndex,
                outputProbIndex = outputProbIndex,
                outputProbType = outputProbTensor.dataType(),
                outputProbBuffer = outputProbBuffer,
                stateOutputBuffer = stateOutputBuffer,
            )
        }
    }

    fun processPcm16(pcm16: ByteArray, length: Int, sampleRate: Int): Float {
        if (length <= 0) {
            return Float.NaN
        }
        pushPcm16(pcm16, length)
        val frame = snapshotFrame()
        val audioBuffer = inputBuffers[audioInputIndex]
        audioBuffer.clear()
        for (value in frame) {
            when (audioInputType) {
                DataType.FLOAT32 -> audioBuffer.putFloat(value)
                DataType.INT16 -> audioBuffer.putShort((value * 32767.0f).toInt().toShort())
                DataType.INT32 -> audioBuffer.putInt((value * 32767.0f).toInt())
                else -> audioBuffer.putFloat(value)
            }
        }
        audioBuffer.rewind()

        sampleRateIndex?.let { idx ->
            val tensor = interpreter.getInputTensor(idx)
            val buffer = inputBuffers[idx]
            buffer.clear()
            when (tensor.dataType()) {
                DataType.INT32 -> buffer.putInt(sampleRate)
                DataType.INT64 -> buffer.putLong(sampleRate.toLong())
                DataType.FLOAT32 -> buffer.putFloat(sampleRate.toFloat())
                else -> buffer.putInt(sampleRate)
            }
            buffer.rewind()
        }

        stateInputIndex?.let { idx ->
            val buffer = stateInputBuffer ?: return Float.NaN
            buffer.rewind()
        }

        outputProbBuffer.clear()
        val outputBuffers = HashMap<Int, Any>()
        outputBuffers[outputProbIndex] = outputProbBuffer
        stateOutputBuffer?.let { buffer ->
            buffer.clear()
            stateOutputIndex?.let { outputBuffers[it] = buffer }
        }

        return try {
            interpreter.runForMultipleInputsOutputs(
                inputObjects,
                outputBuffers
            )
            val prob = outputProbBuffer.readFloat(outputProbType)
            if (stateOutputBuffer != null && stateInputBuffer != null) {
                stateOutputBuffer.rewind()
                stateInputBuffer.rewind()
                stateInputBuffer.put(stateOutputBuffer)
                stateInputBuffer.rewind()
            }
            prob
        } catch (e: Exception) {
            Log.w("SileroVad", "Silero VAD inference failed: ${e.message}")
            Float.NaN
        }
    }

    private fun pushPcm16(pcm16: ByteArray, length: Int) {
        var i = 0
        while (i + 1 < length) {
            val sample = (pcm16[i].toInt() and 0xFF) or (pcm16[i + 1].toInt() shl 8)
            val value = sample.toShort() / 32768.0f
            ring[ringIndex] = value
            ringIndex = (ringIndex + 1) % ring.size
            if (ringIndex == 0) {
                ringFilled = true
            }
            i += 2
        }
    }

    private fun snapshotFrame(): FloatArray {
        val frame = FloatArray(ring.size)
        if (ringFilled) {
            var idx = ringIndex
            for (i in frame.indices) {
                frame[i] = ring[idx]
                idx++
                if (idx == ring.size) {
                    idx = 0
                }
            }
        } else {
            val pad = ring.size - ringIndex
            for (i in 0 until pad) {
                frame[i] = 0.0f
            }
            for (i in 0 until ringIndex) {
                frame[pad + i] = ring[i]
            }
        }
        return frame
    }
}

private fun Tensor.elementCount(): Int {
    val shape = shape()
    if (shape.isEmpty()) {
        return 1
    }
    var count = 1
    for (dim in shape) {
        count *= dim
    }
    return count
}

private fun Tensor.allocateBuffer(): ByteBuffer {
    val size = elementCount() * dataType().byteSize()
    return ByteBuffer.allocateDirect(size).order(ByteOrder.nativeOrder())
}

private fun DataType.byteSize(): Int = when (this) {
    DataType.FLOAT32 -> 4
    DataType.INT32 -> 4
    DataType.INT64 -> 8
    DataType.INT16 -> 2
    DataType.UINT8 -> 1
    DataType.INT8 -> 1
    else -> 4
}

private fun ByteBuffer.readFloat(type: DataType): Float {
    rewind()
    return when (type) {
        DataType.FLOAT32 -> getFloat()
        DataType.INT32 -> getInt().toFloat()
        DataType.INT16 -> getShort().toFloat()
        DataType.INT64 -> getLong().toFloat()
        else -> getFloat()
    }
}
