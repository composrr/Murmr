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
        // The redesign is Inter-only. `font-serif` is kept as an alias so
        // any leftover class still renders in the new type.
        serif: ['Inter', '-apple-system', 'BlinkMacSystemFont', 'Segoe UI', 'system-ui', 'sans-serif'],
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
        accent: 'rgb(var(--accent-rgb) / <alpha-value>)',
        danger: 'rgb(var(--danger-rgb) / <alpha-value>)',
        good: 'rgb(var(--good-rgb) / <alpha-value>)',
        warn: 'rgb(var(--warn-rgb) / <alpha-value>)',
        'border-strong': 'var(--border-strong)',
        'bg-popover': 'var(--bg-popover)',
      },
      borderRadius: {
        window: '14px',
        card: '18px',
        row: '12px',
        control: '9px',
      },
      boxShadow: {
        window: 'var(--shadow-window)',
      },
    },
  },
  plugins: [],
};
