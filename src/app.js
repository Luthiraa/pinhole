const code = document.querySelector('#code');
const connect = document.querySelector('#connect');
const disconnect = document.querySelector('#disconnect');
const join = document.querySelector('#join');
const session = document.querySelector('#session');
const status = document.querySelector('#status');
const shot = document.querySelector('#shot');
const typing = document.querySelector('#typing');
const retry = document.querySelector('#retry');
let secret = '';
let timer;
let busy = false;
let imageUrl;
let inputQueue = Promise.resolve();
let start;
let touchY;

async function request(path, data) {
  const response = await fetch(path, {
    method: 'POST',
    headers: { Authorization: `Bearer ${secret}`, ...(data ? { 'Content-Type': 'application/json' } : {}) },
    body: data ? JSON.stringify(data) : undefined,
    cache: 'no-store'
  });
  if (!response.ok) {
    const error = new Error((await response.text()) || 'Connection failed');
    error.status = response.status;
    throw error;
  }
  return response;
}

async function refresh() {
  if (busy || !secret) return;
  busy = true;
  try {
    const response = await request('/shot');
    const blob = await response.blob();
    if (blob.type !== 'image/png') throw new Error('Invalid screen image');
    const next = URL.createObjectURL(blob);
    shot.src = next;
    if (imageUrl) URL.revokeObjectURL(imageUrl);
    imageUrl = next;
    code.value = '';
    status.textContent = 'Connected';
    retry.hidden = true;
    if (!timer) timer = setInterval(refresh, 1200);
  } catch (error) {
    clearInterval(timer);
    timer = undefined;
    if (error.status === 401 || error.status === 429) {
      secret = '';
      session.hidden = true;
      join.hidden = false;
    } else {
      retry.hidden = false;
    }
    status.textContent = error.message;
  } finally {
    busy = false;
  }
}

function send(data) {
  inputQueue = inputQueue.catch(() => {}).then(() => request('/control', data)).catch(error => {
    status.textContent = error.message;
  });
}

function position(event) {
  const rect = shot.getBoundingClientRect();
  return {
    x: Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width)),
    y: Math.max(0, Math.min(1, (event.clientY - rect.top) / rect.height))
  };
}

connect.addEventListener('click', () => {
  secret = code.value.trim();
  if (!secret) { status.textContent = 'Enter the session code'; return; }
  join.hidden = true;
  session.hidden = false;
  refresh();
});
code.addEventListener('keydown', event => { if (event.key === 'Enter') connect.click(); });
retry.addEventListener('click', refresh);

disconnect.addEventListener('click', () => {
  clearInterval(timer);
  timer = undefined;
  secret = '';
  session.hidden = true;
  join.hidden = false;
  status.textContent = 'Disconnected';
  if (imageUrl) URL.revokeObjectURL(imageUrl);
  imageUrl = undefined;
  shot.removeAttribute('src');
  retry.hidden = true;
});

shot.addEventListener('pointerdown', event => {
  if (event.button !== 0) return;
  event.preventDefault();
  start = position(event);
  shot.setPointerCapture(event.pointerId);
});
shot.addEventListener('pointerup', event => {
  if (!start) return;
  event.preventDefault();
  const end = position(event);
  const distance = Math.hypot(end.x - start.x, end.y - start.y);
  send(distance > 0.015
    ? { action: 'drag', ...start, to_x: end.x, to_y: end.y }
    : { action: 'click', ...end, button: 'left' });
  start = undefined;
});
shot.addEventListener('pointercancel', () => { start = undefined; });
shot.addEventListener('touchstart', event => {
  if (event.touches.length === 2) {
    start = undefined;
    touchY = (event.touches[0].clientY + event.touches[1].clientY) / 2;
  }
}, { passive: false });
shot.addEventListener('touchmove', event => {
  if (event.touches.length !== 2 || touchY === undefined) return;
  event.preventDefault();
  const next = (event.touches[0].clientY + event.touches[1].clientY) / 2;
  if (Math.abs(next - touchY) >= 12) {
    send({ action: 'scroll', ...position(event.touches[0]), delta: Math.max(-10, Math.min(10, Math.round((next - touchY) / 12))) });
    touchY = next;
  }
}, { passive: false });
shot.addEventListener('touchend', () => { touchY = undefined; });
shot.addEventListener('contextmenu', event => {
  event.preventDefault();
  send({ action: 'click', ...position(event), button: 'right' });
});
shot.addEventListener('wheel', event => {
  event.preventDefault();
  send({ action: 'scroll', ...position(event), delta: Math.max(-10, Math.min(10, -Math.sign(event.deltaY) * 3)) });
}, { passive: false });

typing.addEventListener('input', () => {
  if (typing.value) send({ action: 'text', text: typing.value });
  typing.value = '';
});
document.addEventListener('keydown', event => {
  if (!secret || event.target === code || ['Shift', 'Control', 'Alt', 'Meta'].includes(event.key)) return;
  if (event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.altKey) {
    if (event.target === typing) return;
    event.preventDefault();
    send({ action: 'text', text: event.key });
  } else {
    event.preventDefault();
    send({ action: 'key', code: event.code, meta: event.metaKey, ctrl: event.ctrlKey, alt: event.altKey, shift: event.shiftKey });
  }
});
