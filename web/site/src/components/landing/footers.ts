export interface Footer {
  mode: string;
  plan?: boolean;
  model?: string;
  effort?: string;
  audio?: boolean;
}

export const footers: Record<'codex' | 'claude' | 'gemini', Footer> = {
  codex: { mode: 'Auto review', plan: true, model: '6.1 Sol', effort: 'Low' },
  claude: { mode: 'Manual', model: 'Opus 5.5', effort: 'Default' },
  gemini: { mode: 'Default', audio: true },
};
