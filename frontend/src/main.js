import { createApp } from 'vue'
import App from './App.vue'

const app = createApp(App)
app.mount('#app')

// Global CSS custom properties
const style = document.createElement('style')
style.textContent = `
  :root {
    --bg: #f0f2f5;
    --bg-alt: #ffffff;
    --surface: rgba(255, 255, 255, 0.72);
    --surface-hover: rgba(255, 255, 255, 0.9);
    --glass: rgba(255, 255, 255, 0.55);
    --glass-border: rgba(255, 255, 255, 0.3);
    --glass-shadow: 0 8px 32px rgba(0, 0, 0, 0.08);
    --text: #1d1d1f;
    --text-2: #6e6e73;
    --text-3: #aeaeb2;
    --primary: #6366f1;
    --primary-glow: rgba(99, 102, 241, 0.2);
    --success: #34d399;
    --warning: #fbbf24;
    --danger: #ef4444;
    --info: #60a5fa;
    --radius: 16px;
    --radius-sm: 10px;
    --radius-xs: 6px;
    --transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  }
  [data-theme="dark"] {
    --bg: #0a0a0f;
    --bg-alt: #14141f;
    --surface: rgba(30, 30, 50, 0.6);
    --surface-hover: rgba(40, 40, 65, 0.7);
    --glass: rgba(20, 20, 40, 0.5);
    --glass-border: rgba(255, 255, 255, 0.06);
    --glass-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
    --text: #f5f5f7;
    --text-2: #98989d;
    --text-3: #6e6e73;
    --primary-glow: rgba(99, 102, 241, 0.35);
  }
  *, *::before, *::after { box-sizing: border-box; }
  body {
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
    background: var(--bg);
    color: var(--text);
    margin: 0;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }
  input, select, textarea, button { font-family: inherit; }
  ::-webkit-scrollbar { width: 6px; }
  ::-webkit-scrollbar-track { background: transparent; }
  ::-webkit-scrollbar-thumb { background: var(--text-3); border-radius: 3px; }
  ::selection { background: var(--primary); color: white; }
  @keyframes fadeIn { from { opacity: 0; transform: translateY(10px); } to { opacity: 1; transform: translateY(0); } }
  @keyframes slideUp { from { opacity: 0; transform: translateY(20px); } to { opacity: 1; transform: translateY(0); } }
  @keyframes pulse-glow { 0%, 100% { box-shadow: 0 0 20px var(--primary-glow); } 50% { box-shadow: 0 0 40px var(--primary-glow); } }
  @keyframes shimmer { 0% { background-position: -200% 0; } 100% { background-position: 200% 0; } }
`
document.head.appendChild(style)
