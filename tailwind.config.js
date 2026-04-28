/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      fontFamily: {
        sans: [
          'Inter',
          '-apple-system',
          'BlinkMacSystemFont',
          'Segoe UI',
          'system-ui',
          'sans-serif',
        ],
        serif: [
          'Source Serif 4',
          'Source Serif Pro',
          'Georgia',
          'serif',
        ],
      },
      colors: {
        'bg-window': 'var(--bg-window)',
        'bg-chrome': 'var(--bg-chrome)',
        'bg-content': 'var(--bg-content)',
        'bg-row': 'var(--bg-row)',
        'bg-control': 'var(--bg-control)',
        'bg-selected': 'var(--bg-selected)',
        'text-primary': 'var(--text-primary)',
        'text-secondary': 'var(--text-secondary)',
        'text-tertiary': 'var(--text-tertiary)',
        'text-quaternary': 'var(--text-quaternary)',
        'text-on-primary': 'var(--text-on-primary)',
        'border-hairline': 'var(--border-hairline)',
        'border-control': 'var(--border-control)',
        'hud-bg': 'var(--hud-bg)',
        'hud-text': 'var(--hud-text)',
        'hud-recording': 'var(--hud-recording)',
      },
      borderRadius: {
        window: '14px',
        card: '12px',
        row: '10px',
        control: '7px',
      },
      boxShadow: {
        window: 'var(--shadow-window)',
      },
    },
  },
  plugins: [],
};
