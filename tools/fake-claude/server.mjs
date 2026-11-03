// Dev-only scripted fake claude.ai server. Node 22, no dependencies.
//   node tools/fake-claude/server.mjs [--port 8787]
import http from 'node:http';

// Must equal FAKE_ORG_UUID in crates/limits-core/src/session/mod.rs.
export const ORG_PERSONAL = '0b5a1f6e-3c2d-4e8a-9a41-7d6c5b4a3f21';
export const ORG_TEAM = '7c9e2d84-51ab-4f06-8b3d-2e1f0a9c8d57';
export const ORG_API = 'a3d41c70-96e2-4b58-b1f7-5c08e6d29a34';

const MIN = 60_000, HOUR = 3_600_000, DAY = 24 * HOUR;
const SESSION_LEN = 5 * HOUR, WEEKLY_LEN = 7 * DAY;

const portArg = process.argv.indexOf('--port');
const PORT = portArg > 0 ? Number(process.argv[portArg + 1]) : 8787;

const t0 = Date.now();
const state = {
  session: { utilization: 62, resetsAt: t0 + 2.5 * HOUR }, // null = no active window
  weekly: { utilization: 34, resetsAt: t0 + 3.5 * DAY },
  using: false,
  perMinute: 6,
  lastTick: t0,
  twoOrgs: false,
  failures: [], // queue of 'fail' | '401' | '403' | 'unknown'
};

const win = (w) => (w ? { utilization: w.utilization, resets_at: new Date(w.resetsAt).toISOString() } : null);

// Lazy time-based update, run on every GET.
function tick(now = Date.now()) {
  for (const key of ['session', 'weekly']) {
    if (state[key] && state[key].resetsAt <= now) state[key] = null;
  }
  if (state.using) {
    const minutes = (now - state.lastTick) / MIN;
    const inc = state.perMinute * minutes;
    if (inc > 0) {
      if (!state.session) state.session = { utilization: 0, resetsAt: now + SESSION_LEN };
      if (!state.weekly) state.weekly = { utilization: 0, resetsAt: now + WEEKLY_LEN };
      state.session.utilization = Math.min(100, state.session.utilization + inc);
      state.weekly.utilization = Math.min(100, state.weekly.utilization + inc / 10);
    }
  }
  state.lastTick = now;
}

function orgs() {
  const list = [{ uuid: ORG_PERSONAL, name: 'Max plan · Personal', capabilities: ['chat', 'claude_max'], rate_limit_tier: 'default_claude_max_5x' }];
  if (state.twoOrgs) list.push({ uuid: ORG_TEAM, name: 'Team · Acme', capabilities: ['chat', 'claude_team'], rate_limit_tier: 'default_claude_team' });
  list.push({ uuid: ORG_API, name: 'API Org', capabilities: ['api'], rate_limit_tier: 'auto_prepaid_tier_0' });
  return list;
}

const usage = () => ({ five_hour: win(state.session), seven_day: win(state.weekly), limits: [] });

const ctlState = () => ({
  ...usage(),
  using: state.using,
  perMinute: state.perMinute,
  twoOrgs: state.twoOrgs,
  pendingFailures: [...state.failures],
  now: new Date().toISOString(),
});

const ACTIONS = {
  'use-claude': (q) => {
    tick();
    const n = Number(q.get('n'));
    if (n > 0) state.perMinute = n;
    state.using = !state.using;
  },
  jump: () => {
    tick();
    const at = Date.now() + 45_000;
    if (!state.session) state.session = { utilization: 62, resetsAt: at };
    else state.session.resetsAt = at;
  },
  'surprise-session': () => { state.session = null; },
  'surprise-weekly': () => { state.weekly = null; },
  'surprise-both': () => { state.session = null; state.weekly = null; },
  'go-92': () => {
    tick();
    if (!state.session) state.session = { utilization: 0, resetsAt: Date.now() + SESSION_LEN };
    state.session.utilization = 92;
  },
  'fail-503': () => { state.failures.push('fail', 'fail', 'fail'); },
  'next-401': () => { state.failures.push('401'); },
  'next-403': () => { state.failures.push('403'); },
  unknown: () => { state.failures.push('unknown'); },
  'orgs-1': () => { state.twoOrgs = false; },
  'orgs-2': () => { state.twoOrgs = true; },
  'toggle-orgs': () => { state.twoOrgs = !state.twoOrgs; },
};

const LABELS = [
  ['use-claude', 'use Claude (+N%/min, toggle)'],
  ['jump', 'jump to 45 s before session reset'],
  ['surprise-session', 'surprise reset session'],
  ['surprise-weekly', 'surprise reset weekly'],
  ['surprise-both', 'surprise reset both'],
  ['go-92', 'go 92%'],
  ['fail-503', 'fail next 3 (503)'],
  ['next-401', 'next is 401'],
  ['next-403', 'next is Cloudflare 403'],
  ['unknown', 'unknown shape'],
  ['orgs-1', '1 org'],
  ['orgs-2', '2 orgs'],
];

const PAGE = `<!doctype html><meta charset="utf-8"><title>fake claude.ai</title>
<style>body{font:14px system-ui;margin:20px;max-width:560px}button{margin:3px;padding:6px 10px}pre{background:#eee;padding:8px}</style>
<h3>fake claude.ai control</h3>
<label>N %/min <input id="n" type="number" value="6" min="1" style="width:4em"></label><br>
${LABELS.map(([a, l]) => `<button onclick="act('${a}')">${l}</button>`).join('')}
<pre id="s"></pre>
<script>
async function act(a){const n=document.getElementById('n').value;await fetch('/_ctl/'+a+(a==='use-claude'?'?n='+n:''),{method:'POST'});refresh()}
async function refresh(){const r=await fetch('/_ctl/state');document.getElementById('s').textContent=JSON.stringify(await r.json(),null,2)}
refresh();setInterval(refresh,2000);
</script>`;

function send(res, status, body, type = 'application/json') {
  res.writeHead(status, { 'content-type': type });
  res.end(typeof body === 'string' ? body : JSON.stringify(body));
}

// Scripted failure for /api requests; returns true if it handled the response.
function failure(res) {
  const f = state.failures.shift();
  if (!f) return false;
  if (f === 'fail') send(res, 503, 'Service Unavailable', 'text/plain');
  else if (f === '401') send(res, 401, { error: { type: 'authentication_error', message: 'Invalid authorization' } });
  else if (f === '403') send(res, 403, '<html><head><title>Just a moment...</title></head><body>cf-chl challenge</body></html>', 'text/html');
  else send(res, 200, { hello: 'world' });
  return true;
}

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://${req.headers.host}`);
  const path = url.pathname.replace(/\/+$/, '') || '/';
  if (req.method === 'GET' && path === '/_ctl') return send(res, 200, PAGE, 'text/html; charset=utf-8');
  if (req.method === 'GET' && path === '/_ctl/state') { tick(); return send(res, 200, ctlState()); }
  const m = path.match(/^\/_ctl\/([\w-]+)$/);
  if (req.method === 'POST' && m) {
    const fn = ACTIONS[m[1]];
    if (!fn) return send(res, 404, { error: 'unknown action' });
    fn(url.searchParams);
    return send(res, 200, ctlState());
  }
  if (req.method === 'GET' && path.startsWith('/api/organizations')) {
    if (failure(res)) return;
    if (path === '/api/organizations') return send(res, 200, orgs());
    if (/^\/api\/organizations\/[^/]+\/usage$/.test(path)) {
      tick();
      return send(res, 200, usage());
    }
  }
  send(res, 404, { error: 'not found' });
});

server.listen(PORT, '127.0.0.1', () => console.log(`fake claude.ai on http://127.0.0.1:${PORT}  (control: /_ctl)`));
