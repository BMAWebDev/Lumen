// Custom icons, drawn with simple lines (stroke = currentColor).
const svg = (body) =>
  `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;

export const icons = {
  home: svg('<path d="M4 11.2 12 4.5l8 6.7"/><path d="M6.5 9.5V19h11V9.5"/><path d="M10 19v-5h4v5"/>'),
  library: svg('<rect x="4" y="4" width="6.5" height="6.5" rx="1.4"/><rect x="13.5" y="4" width="6.5" height="6.5" rx="1.4"/><rect x="4" y="13.5" width="6.5" height="6.5" rx="1.4"/><rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.4"/>'),
  close: svg('<circle cx="12" cy="12" r="8"/><path d="m9.2 9.2 5.6 5.6M14.8 9.2l-5.6 5.6"/>'),
  refresh: svg('<path d="M19 12a7 7 0 1 1-2.05-4.95"/><path d="M19 5v4h-4"/>'),
  gear: svg('<circle cx="12" cy="12" r="2.8"/><path d="M12 3.5v2.2M12 18.3v2.2M20.5 12h-2.2M5.7 12H3.5M18 6l-1.6 1.6M7.6 16.4 6 18M18 18l-1.6-1.6M7.6 7.6 6 6"/>'),
  power: svg('<path d="M12 4v7"/><path d="M7.1 7.2a7 7 0 1 0 9.8 0"/>'),
  more: svg('<circle cx="6" cy="12" r="1.2" fill="currentColor"/><circle cx="12" cy="12" r="1.2" fill="currentColor"/><circle cx="18" cy="12" r="1.2" fill="currentColor"/>'),
  folder: svg('<path d="M3.5 7.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"/>'),
  gamepad: svg('<path d="M7.5 7h9a4.5 4.5 0 0 1 4.4 5.4l-.8 4a2.2 2.2 0 0 1-3.8 1L14.5 15h-5l-1.8 2.4a2.2 2.2 0 0 1-3.8-1l-.8-4A4.5 4.5 0 0 1 7.5 7z"/><path d="M8 10v3M6.5 11.5h3"/><circle cx="15.5" cy="10.8" r=".6" fill="currentColor"/><circle cx="17" cy="12.6" r=".6" fill="currentColor"/>'),
  play: svg('<path d="M8 5.5v13l10-6.5z" fill="currentColor" stroke="none"/>'),
};
