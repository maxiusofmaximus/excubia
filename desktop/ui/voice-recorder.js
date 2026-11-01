// Microphone capture with Web Audio. The blocks stay in memory: nothing is written to disk
// or to a cache, and the chat module drops them once they are sent.

// shortcut: ScriptProcessorNode is deprecated, but it runs in WebView2 without a worklet file, which the CSP would have to allow; upgrade to an AudioWorklet when the CSP allows it.
export async function startCapture(onBlock) {
  const stream = await navigator.mediaDevices.getUserMedia({ audio: { channelCount: 1 } });
  const context = new AudioContext();
  const source = context.createMediaStreamSource(stream);
  const processor = context.createScriptProcessor(4096, 1, 1);
  // The processor only runs while it is connected to the output; a silent gain keeps the mic from echoing.
  const mute = context.createGain();
  mute.gain.value = 0;
  processor.onaudioprocess = (event) => onBlock(new Float32Array(event.inputBuffer.getChannelData(0)));
  source.connect(processor);
  processor.connect(mute);
  mute.connect(context.destination);
  return {
    rate: context.sampleRate,
    async stop() {
      processor.onaudioprocess = null;
      source.disconnect();
      processor.disconnect();
      mute.disconnect();
      for (const track of stream.getTracks()) track.stop();
      await context.close();
    },
  };
}
