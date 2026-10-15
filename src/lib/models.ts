/**
 * Transcription models offered in the pill dropdown and the settings window.
 * `local:*` ids run on-device with faster-whisper (see `src-tauri/src/local.rs`);
 * the rest go to the OpenAI API.
 */
export interface ModelOption {
  id: string;
  label: string;
  local: boolean;
}

export const MODELS: ModelOption[] = [
  { id: "whisper-1", label: "whisper-1", local: false },
  { id: "gpt-4o-transcribe", label: "gpt-4o-transcribe", local: false },
  { id: "gpt-4o-mini-transcribe", label: "gpt-4o-mini-transcribe", local: false },
  { id: "local:small", label: "faster-whisper small", local: true },
  { id: "local:medium", label: "faster-whisper medium", local: true },
];
