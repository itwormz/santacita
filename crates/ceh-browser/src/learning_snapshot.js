() => {
  // Learning mode (developer only). Read only: never focus, click, scroll or inject a listener.
  // Records the STRUCTURE of the page. It never reads what a person has typed: the only value it reads is an
  // option's own (page data, marked at the end of its line); a button's caption comes from its attribute.
  if (location.origin !== 'https://icp.administracionelectronica.gob.es') return null;
  const plain = el => (el.textContent || '').replace(/\s+/g, ' ').trim().slice(0, 300);
  const clean = text => String(text || '').replace(/\s+/g, ' ').trim().slice(0, 300);
  const stripped = url => String(url || '').split('#')[0].split('?')[0].replace(/;[^/]*/g, '');
  const skip = el => !!el.closest('script,style,select,textarea,template,noscript');
  const text = [];
  for (const el of document.querySelectorAll('p,li,td,th,label,legend,button,h1,h2,h3')) {
    if (text.length >= 100) break;
    if (skip(el)) continue;
    const line = plain(el);
    if (line && !text.includes(line)) text.push(line);
  }
  const headings = [];
  for (const el of document.querySelectorAll('h1,h2,h3,legend')) {
    if (headings.length >= 50) break;
    const line = plain(el);
    if (line) headings.push(line);
  }
  const forms = Array.from(document.forms).slice(0, 50).map(form => ({
    id: clean(form.id), name: clean(form.getAttribute('name')),
    action: stripped(form.getAttribute('action')), method: clean(form.getAttribute('method') || 'get').toLowerCase(),
  }));
  const labelOf = el => {
    if (el.id) {
      const byFor = Array.from(document.querySelectorAll('label[for]')).find(l => l.getAttribute('for') === el.id);
      if (byFor) return plain(byFor);
    }
    const wrapping = el.closest('label');
    if (wrapping) return plain(wrapping);
    const type = (el.getAttribute('type') || '').toLowerCase();
    if (['submit', 'button', 'reset', 'image'].includes(type)) return clean(el.getAttribute('value'));
    return clean(el.getAttribute('aria-label') || el.getAttribute('placeholder') || el.getAttribute('title'));
  };
  const controls = [];
  for (const el of document.querySelectorAll('input,select,textarea,button')) {
    if (controls.length >= 400) break;
    const tag = el.localName;
    const kind = tag === 'input' ? (el.getAttribute('type') || 'text').toLowerCase() : tag;
    const options = tag === 'select'
      ? Array.from(el.options).slice(0, 500).map(o => ({value: o.value, label: clean(o.text), selected: o.selected})) // option-value
      : [];
    controls.push({
      tag, kind, id: clean(el.id), name: clean(el.getAttribute('name')),
      label: kind === 'hidden' ? '' : labelOf(el),
      form: el.form ? stripped(el.form.getAttribute('action')) : '',
      disabled: el.disabled === true, options,
    });
  }
  return JSON.stringify({
    path: stripped(location.pathname), load: String(performance.timeOrigin), title: clean(document.title), headings, text, forms, controls,
  });
}
