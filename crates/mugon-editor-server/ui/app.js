import { Viewer } from './viewer.js';

const $ = (id) => document.getElementById(id);
const PAGE = 120;
const FILTER_KEYS = ['q', 'pack', 'category', 'tag', 'kind'];
const state = { q: '', pack: '', category: '', tag: '', kind: '', offset: 0, total: 0, loading: false, selected: null };
let viewer = null;
let generation = 0;

const api = (path, opts) => fetch(path, opts).then((r) => (r.ok ? r.json() : r.text().then((t) => Promise.reject(new Error(t)))));

function query(extra = {}) {
  const p = new URLSearchParams();
  for (const k of FILTER_KEYS) if (state[k]) p.set(k, state[k]);
  p.set('offset', state.offset);
  for (const [k, v] of Object.entries(extra)) p.set(k, v);
  return p.toString();
}

function facetList(el, items, key, allLabel) {
  el.innerHTML = '';
  if (allLabel) {
    items = [{ name: '', label: allLabel, count: items.reduce((n, f) => n + f.count, 0) }, ...items];
  }
  for (const f of items) {
    const li = document.createElement('li');
    li.className = state[key] === f.name ? 'on' : '';
    const label = document.createElement('span');
    label.textContent = f.label ?? f.name;
    const count = document.createElement('small');
    count.textContent = f.count;
    li.append(label, count);
    li.onclick = () => { state[key] = state[key] === f.name ? '' : f.name; refresh(); };
    el.appendChild(li);
  }
}

async function refreshFacets() {
  const packQ = state.pack ? `pack=${encodeURIComponent(state.pack)}&` : '';
  const [packs, cats, tags] = await Promise.all([api('/api/packs'), api('/api/categories?' + packQ), api('/api/tags?' + packQ + 'min=3')]);
  facetList($('packs'), packs, 'pack', 'All');
  facetList($('categories'), cats, 'category');
  const box = $('tags');
  box.innerHTML = '';
  for (const t of tags.slice(0, 60)) {
    const c = document.createElement('span');
    c.className = 'chip' + (state.tag === t.name ? ' on' : '');
    c.textContent = `${t.name} ${t.count}`;
    c.onclick = () => { state.tag = state.tag === t.name ? '' : t.name; refresh(); };
    box.appendChild(c);
  }
}

function card(a) {
  const el = document.createElement('div');
  el.className = 'card';
  el.dataset.id = a.id;
  const img = document.createElement('div');
  img.className = 'img';
  if (a.thumbnail) {
    const i = document.createElement('img');
    i.loading = 'lazy';
    i.src = `/lib/${a.pack}/${a.thumbnail}`;
    img.appendChild(i);
  } else img.textContent = 'no preview';
  const name = document.createElement('div');
  name.className = 'name';
  name.textContent = name.title = a.name;
  const sub = document.createElement('div');
  sub.className = 'sub';
  sub.textContent = `${a.pack} · ${a.category}`;
  el.append(img, name, sub);
  el.onclick = () => select(a);
  return el;
}

async function loadPage(reset) {
  if (reset) { generation++; state.offset = 0; state.total = 0; state.loading = false; $('grid').innerHTML = ''; }
  else if (state.loading || (state.offset >= state.total && state.total)) return;
  state.loading = true;
  const gen = generation;
  const res = await api('/api/assets?' + query({ limit: PAGE }));
  if (gen !== generation) return;
  state.total = res.total;
  state.offset += res.assets.length;
  for (const a of res.assets) $('grid').appendChild(card(a));
  $('count').textContent = `${res.total} assets`;
  state.loading = false;
  if (state.offset < state.total && $('sentinel').getBoundingClientRect().top < innerHeight + 200) loadPage(false);
}

const refresh = () => Promise.all([refreshFacets(), loadPage(true)]);

async function select(a) {
  state.selected = a;
  document.querySelectorAll('.card.sel').forEach((c) => c.classList.remove('sel'));
  document.querySelector(`.card[data-id="${CSS.escape(a.id)}"]`)?.classList.add('sel');
  $('detail').hidden = false;
  if (!viewer) viewer = new Viewer($('viewer'));
  viewer.resize();
  $('d-name').textContent = a.name;
  const b = a.bounds;
  const size = [0, 1, 2].map((i) => (b.max[i] - b.min[i]).toFixed(2));
  const meta = $('d-meta');
  meta.innerHTML = '';
  for (const [k, v] of [['Pack', a.pack], ['Category', a.category], ['Type', a.kind], ['Triangles', a.triangles], ['Size (x y z)', size.join(' × ') + ' m'], ['Model', a.model], ['Texture', a.texture || '–']]) {
    const dt = document.createElement('dt'), dd = document.createElement('dd');
    dt.textContent = k; dd.textContent = v;
    meta.append(dt, dd);
  }
  renderTags(a);
  $('d-category').value = a.category;
  const variant = $('variant');
  variant.innerHTML = '';
  const opts = a.texture ? [{ name: 'default', texture: a.texture }, ...a.variants] : [];
  for (const v of opts) variant.add(new Option(v.name, v.texture));
  variant.hidden = opts.length < 2;
  const clip = $('clip');
  clip.innerHTML = '';
  clip.add(new Option('no animation', -1));
  a.animations.forEach((n, i) => clip.add(new Option(n, i)));
  clip.hidden = !a.animations.length;
  try {
    await viewer.load(`/lib/${a.pack}/${a.model}`);
    viewer.setBounds($('boundsbox').checked);
  } catch (e) { console.error(e); }
}

function renderTags(a) {
  const box = $('d-tags');
  box.innerHTML = '';
  for (const t of a.tags) {
    const c = document.createElement('span');
    c.className = 'chip';
    c.textContent = t;
    const x = document.createElement('span');
    x.className = 'x'; x.textContent = '×'; x.title = 'remove tag';
    x.onclick = async (e) => { e.stopPropagation(); await saveOverride({ remove_tags: [t] }); };
    c.appendChild(x);
    c.onclick = () => { state.tag = t; refresh(); };
    box.appendChild(c);
  }
}

async function saveOverride(patch) {
  const a = state.selected;
  const body = { id: a.id, category: null, add_tags: [], remove_tags: [], hidden: false, ...patch };
  await api('/api/override', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
  Object.assign(a, await api('/api/asset?id=' + encodeURIComponent(a.id)));
  renderTags(a);
  refreshFacets();
}

let timer;
$('search').oninput = (e) => { clearTimeout(timer); timer = setTimeout(() => { state.q = e.target.value; refresh(); }, 150); };
$('kind').onchange = (e) => { state.kind = e.target.value; refresh(); };
$('close').onclick = () => { $('detail').hidden = true; };
addEventListener('keydown', (e) => { if (e.key === 'Escape') $('detail').hidden = true; });
$('variant').onchange = (e) => viewer.setTexture(`/lib/${state.selected.pack}/${e.target.value}`);
$('clip').onchange = (e) => viewer.play(+e.target.value);
$('wire').onchange = (e) => viewer.setWireframe(e.target.checked);
$('boundsbox').onchange = (e) => viewer.setBounds(e.target.checked);
$('d-save').onclick = async () => {
  const newTag = $('d-newtag').value.trim().toLowerCase();
  const cat = $('d-category').value.trim();
  await saveOverride({ add_tags: newTag ? [newTag] : [], category: cat && cat !== state.selected.category ? cat : null });
  $('d-newtag').value = '';
};
$('d-copy').onclick = () => navigator.clipboard.writeText(`${state.selected.pack}/${state.selected.model}`);
$('d-editor').onclick = async () => {
  try { await api('/api/editor/open', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ asset: state.selected.id }) }); }
  catch (e) { alert(e.message); }
};
new IntersectionObserver((es) => { if (es[0].isIntersecting) loadPage(false); }, { root: $('grid-wrap') }).observe($('sentinel'));

const info = await api('/api/info');
$('d-editor').hidden = !info.editor;
refresh();
