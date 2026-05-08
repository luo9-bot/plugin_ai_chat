let token = localStorage.getItem('admin_token') || ''

export function getToken() { return token }
export function setToken(t) { token = t; localStorage.setItem('admin_token', t) }
export function clearToken() { token = ''; localStorage.removeItem('admin_token') }

export function headers() {
  return { 'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json' }
}

export async function api(path, opt = {}) {
  const r = await fetch(path, { headers: headers(), ...opt })
  if (r.status === 401) { clearToken(); throw new Error('unauthorized') }
  const j = await r.json()
  if (!r.ok) throw new Error(j.error || 'request failed')
  return j
}

export async function tryLogin(t) {
  const r = await fetch('/api/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ token: t })
  })
  if (r.ok) { setToken(t); return true }
  return false
}
