// Custom icons, drawn with simple lines (stroke = currentColor).
const svg = (body) =>
  `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;

export const icons = {
  home: svg('<path d="M4 11.2 12 4.5l8 6.7"/><path d="M6.5 9.5V19h11V9.5"/><path d="M10 19v-5h4v5"/>'),
  library: svg('<rect x="4" y="4" width="6.5" height="6.5" rx="1.4"/><rect x="13.5" y="4" width="6.5" height="6.5" rx="1.4"/><rect x="4" y="13.5" width="6.5" height="6.5" rx="1.4"/><rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.4"/>'),
  close: svg('<circle cx="12" cy="12" r="8"/><path d="m9.2 9.2 5.6 5.6M14.8 9.2l-5.6 5.6"/>'),
  refresh: svg('<path d="M19 12a7 7 0 1 1-2.05-4.95"/><path d="M19 5v4h-4"/>'),
  gear: svg('<path d="M10.54 4.95 L10.92 2.86 L13.08 2.86 L13.46 4.95 L15.95 5.98 L17.70 4.78 L19.22 6.30 L18.02 8.05 L19.05 10.54 L21.14 10.92 L21.14 13.08 L19.05 13.46 L18.02 15.95 L19.22 17.70 L17.70 19.22 L15.95 18.02 L13.46 19.05 L13.08 21.14 L10.92 21.14 L10.54 19.05 L8.05 18.02 L6.30 19.22 L4.78 17.70 L5.98 15.95 L4.95 13.46 L2.86 13.08 L2.86 10.92 L4.95 10.54 L5.98 8.05 L4.78 6.30 L6.30 4.78 L8.05 5.98Z"/><circle cx="12" cy="12" r="3"/>'),
  power: svg('<path d="M12 4v7"/><path d="M7.1 7.2a7 7 0 1 0 9.8 0"/>'),
  more: svg('<circle cx="6" cy="12" r="1.2" fill="currentColor"/><circle cx="12" cy="12" r="1.2" fill="currentColor"/><circle cx="18" cy="12" r="1.2" fill="currentColor"/>'),
  folder: svg('<path d="M3.5 7.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"/>'),
  gamepad: svg('<path d="M7.5 7h9a4.5 4.5 0 0 1 4.4 5.4l-.8 4a2.2 2.2 0 0 1-3.8 1L14.5 15h-5l-1.8 2.4a2.2 2.2 0 0 1-3.8-1l-.8-4A4.5 4.5 0 0 1 7.5 7z"/><path d="M8 10v3M6.5 11.5h3"/><circle cx="15.5" cy="10.8" r=".6" fill="currentColor"/><circle cx="17" cy="12.6" r=".6" fill="currentColor"/>'),
  bluetooth: svg('<path d="m7 7.5 10 9-5 4.5v-17l5 4.5-10 9"/>'),
  speaker: svg('<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z"/><path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11"/>'),
  monitor: svg('<rect x="3.5" y="5" width="17" height="11.5" rx="1.6"/><path d="M9 20h6M12 16.5V20"/>'),
  palette: svg('<path d="M12 4a8 8 0 1 0 0 16c1.3 0 1.8-.9 1.4-1.8-.5-1 .1-2.2 1.3-2.2H17a3 3 0 0 0 3-3A8 8 0 0 0 12 4z"/><circle cx="8" cy="11" r="1"/><circle cx="11" cy="8" r="1"/><circle cx="15" cy="9" r="1"/>'),
  play: svg('<path d="M8 5.5v13l10-6.5z" fill="currentColor" stroke="none"/>'),
};
