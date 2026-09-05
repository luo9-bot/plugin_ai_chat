import { createApp } from 'vue'
import App from './App.vue'

const app = createApp(App)
app.mount('#app')

const style = document.createElement('style')
style.textContent = `
  /* 灵魂架构 v2 · 暖陶土色系（§15 她的房间） */
  :root {
    --bg: #F7F4EF;
    --bg-alt: #FDFCF9;
    --surface: rgba(253, 252, 249, 0.85);
    --surface-hover: rgba(253, 252, 249, 0.98);
    --surface-solid: #FDFCF9;
    --glass: rgba(253, 252, 249, 0.7);
    --glass-border: rgba(61, 57, 41, 0.08);
    --glass-shadow: 0 1px 3px rgba(61, 57, 41, 0.05), 0 1px 2px rgba(61, 57, 41, 0.07);
    --glass-shadow-lg: 0 4px 12px rgba(61, 57, 41, 0.08), 0 1px 3px rgba(61, 57, 41, 0.10);
    --text: #3D3929;
    --text-2: #8A8375;
    --text-3: #B0A99A;
    --primary: #B4703F;
    --primary-hover: #9E5E31;
    --primary-subtle: rgba(180, 112, 63, 0.08);
    --primary-glow: rgba(180, 112, 63, 0.18);
    --accent: #C15F3C;
    --accent-subtle: rgba(193, 95, 60, 0.08);
    --success: #7D9455;
    --success-subtle: rgba(125, 148, 85, 0.10);
    --warning: #B8862B;
    --warning-subtle: rgba(184, 134, 43, 0.10);
    --danger: #A94438;
    --danger-subtle: rgba(169, 68, 56, 0.10);
    --info: #7189A0;
    --info-subtle: rgba(113, 137, 160, 0.10);
    --border: #E8E2D9;
    --border-light: #F0EBE2;
    --radius: 14px;
    --radius-sm: 12px;
    --radius-xs: 10px;
    --radius-full: 9999px;
    --transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
    --transition-fast: all 0.15s cubic-bezier(0.4, 0, 0.2, 1);
  }
  [data-theme="dark"] {
    --bg: #211C16;
    --bg-alt: #292319;
    --surface: rgba(41, 35, 25, 0.85);
    --surface-hover: rgba(41, 35, 25, 0.98);
    --surface-solid: #292319;
    --glass: rgba(33, 28, 22, 0.7);
    --glass-border: rgba(247, 244, 239, 0.06);
    --glass-shadow: 0 1px 3px rgba(0, 0, 0, 0.25), 0 1px 2px rgba(0, 0, 0, 0.35);
    --glass-shadow-lg: 0 4px 12px rgba(0, 0, 0, 0.35), 0 1px 3px rgba(0, 0, 0, 0.45);
    --text: #EFE9DD;
    --text-2: #A69D8B;
    --text-3: #7C7466;
    --primary: #CE8A57;
    --primary-hover: #DA9A6A;
    --primary-subtle: rgba(206, 138, 87, 0.14);
    --primary-glow: rgba(206, 138, 87, 0.24);
    --accent: #D0714B;
    --accent-subtle: rgba(208, 113, 75, 0.14);
    --success: #97AE6E;
    --success-subtle: rgba(151, 174, 110, 0.14);
    --warning: #D2A754;
    --warning-subtle: rgba(210, 167, 84, 0.14);
    --danger: #D0684F;
    --danger-subtle: rgba(208, 104, 79, 0.14);
    --info: #93A6C0;
    --info-subtle: rgba(147, 166, 192, 0.14);
    --border: #3B342A;
    --border-light: #332D24;
  }
  *, *::before, *::after { box-sizing: border-box; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
    background: var(--bg);
    color: var(--text);
    margin: 0;
    line-height: 1.5;
    font-variant-numeric: tabular-nums;
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
