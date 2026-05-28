import { createApp } from 'vue'
import App from './App.vue'

const app = createApp(App)
app.mount('#app')

const style = document.createElement('style')
style.textContent = `
  :root {
    --bg: #f8fafc;
    --bg-alt: #ffffff;
    --surface: rgba(255, 255, 255, 0.85);
    --surface-hover: rgba(255, 255, 255, 0.95);
    --surface-solid: #ffffff;
    --glass: rgba(255, 255, 255, 0.7);
    --glass-border: rgba(0, 0, 0, 0.06);
    --glass-shadow: 0 1px 3px rgba(0, 0, 0, 0.04), 0 1px 2px rgba(0, 0, 0, 0.06);
    --glass-shadow-lg: 0 4px 12px rgba(0, 0, 0, 0.06), 0 1px 3px rgba(0, 0, 0, 0.08);
    --text: #0f172a;
    --text-2: #64748b;
    --text-3: #94a3b8;
    --primary: #10b981;
    --primary-hover: #059669;
    --primary-subtle: rgba(16, 185, 129, 0.08);
    --primary-glow: rgba(16, 185, 129, 0.15);
    --accent: #6366f1;
    --accent-subtle: rgba(99, 102, 241, 0.08);
    --success: #10b981;
    --success-subtle: rgba(16, 185, 129, 0.08);
    --warning: #f59e0b;
    --warning-subtle: rgba(245, 158, 11, 0.08);
    --danger: #ef4444;
    --danger-subtle: rgba(239, 68, 68, 0.08);
    --info: #3b82f6;
    --info-subtle: rgba(59, 130, 246, 0.08);
    --border: #e2e8f0;
    --border-light: #f1f5f9;
    --radius: 12px;
    --radius-sm: 8px;
    --radius-xs: 6px;
    --radius-full: 9999px;
    --transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
    --transition-fast: all 0.15s cubic-bezier(0.4, 0, 0.2, 1);
  }
  [data-theme="dark"] {
    --bg: #0c1222;
    --bg-alt: #111827;
    --surface: rgba(30, 41, 59, 0.7);
    --surface-hover: rgba(30, 41, 59, 0.9);
    --surface-solid: #1e293b;
    --glass: rgba(15, 23, 42, 0.6);
    --glass-border: rgba(255, 255, 255, 0.06);
    --glass-shadow: 0 1px 3px rgba(0, 0, 0, 0.2), 0 1px 2px rgba(0, 0, 0, 0.3);
    --glass-shadow-lg: 0 4px 12px rgba(0, 0, 0, 0.3), 0 1px 3px rgba(0, 0, 0, 0.4);
    --text: #f1f5f9;
    --text-2: #94a3b8;
    --text-3: #64748b;
    --primary-glow: rgba(16, 185, 129, 0.2);
    --primary-subtle: rgba(16, 185, 129, 0.12);
    --accent-subtle: rgba(99, 102, 241, 0.12);
    --success-subtle: rgba(16, 185, 129, 0.12);
    --warning-subtle: rgba(245, 158, 11, 0.12);
    --danger-subtle: rgba(239, 68, 68, 0.12);
    --info-subtle: rgba(59, 130, 246, 0.12);
    --border: #1e293b;
    --border-light: #1e293b;
  }
  *, *::before, *::after { box-sizing: border-box; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
    background: var(--bg);
    color: var(--text);
    margin: 0;
    line-height: 1.5;
  }
  input, select, textarea, button { font-family: inherit; }
  ::-webkit-scrollbar { width: 5px; height: 5px; }
  ::-webkit-scrollbar-track { background: transparent; }
  ::-webkit-scrollbar-thumb { background: var(--text-3); border-radius: 3px; }
  ::-webkit-scrollbar-thumb:hover { background: var(--text-2); }
  ::selection { background: var(--primary); color: white; }
  @keyframes fadeIn { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: translateY(0); } }
  @keyframes slideUp { from { opacity: 0; transform: translateY(16px); } to { opacity: 1; transform: translateY(0); } }

  /* Global card style */
  .card {
    padding: 18px; border-radius: var(--radius);
    background: var(--surface-solid);
    border: 1px solid var(--border);
    box-shadow: var(--glass-shadow);
    transition: var(--transition);
    margin-bottom: 16px;
  }
  .card:hover { box-shadow: var(--glass-shadow-lg); }
  .card-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 14px; }
  .card-header h3 { font-size: 14px; font-weight: 600; display: flex; align-items: center; gap: 8px; }

  /* Common form elements */
  .btn {
    display: inline-flex; align-items: center; justify-content: center; gap: 6px;
    padding: 7px 14px; border: none; border-radius: var(--radius-sm);
    font-size: 13px; font-weight: 500; cursor: pointer;
    transition: var(--transition-fast);
  }
  .btn-primary { background: var(--primary); color: white; }
  .btn-primary:hover { background: var(--primary-hover); }
  .btn-secondary { background: var(--primary-subtle); color: var(--primary); }
  .btn-secondary:hover { background: var(--primary-glow); }
  .btn-danger { background: var(--danger-subtle); color: var(--danger); }
  .btn-danger:hover { background: var(--danger); color: white; }
  .btn-ghost { background: transparent; color: var(--text-2); }
  .btn-ghost:hover { background: var(--primary-subtle); color: var(--text); }
  .btn-sm { padding: 4px 10px; font-size: 12px; }

  .input {
    padding: 7px 12px; border-radius: var(--radius-sm);
    border: 1px solid var(--border); background: var(--bg);
    color: var(--text); font-size: 13px; outline: none;
    transition: var(--transition-fast);
  }
  .input:focus { border-color: var(--primary); box-shadow: 0 0 0 3px var(--primary-glow); }
  .input-sm { padding: 4px 8px; font-size: 12px; }

  .tag {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 2px 8px; border-radius: var(--radius-full);
    font-size: 11px; font-weight: 500;
  }
  .tag-primary { background: var(--primary-subtle); color: var(--primary); }
  .tag-info { background: var(--info-subtle); color: var(--info); }
  .tag-warning { background: var(--warning-subtle); color: var(--warning); }
  .tag-danger { background: var(--danger-subtle); color: var(--danger); }
  .tag-success { background: var(--success-subtle); color: var(--success); }

  .table { width: 100%; border-collapse: collapse; font-size: 13px; }
  .table th { text-align: left; padding: 8px 12px; font-weight: 600; color: var(--text-2); border-bottom: 1px solid var(--border); font-size: 12px; }
  .table td { padding: 8px 12px; border-bottom: 1px solid var(--border-light); }
  .table tr:hover td { background: var(--primary-subtle); }

  .empty { text-align: center; padding: 32px; color: var(--text-3); font-size: 13px; }
  .loading { text-align: center; padding: 32px; color: var(--text-3); font-size: 13px; }
`
document.head.appendChild(style)
