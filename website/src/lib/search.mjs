export function searchPages(pages, {q = '', state = 'current', role = '', audience = ''} = {}) {
  const words = q.toLowerCase().trim().split(/\s+/).filter(Boolean);
  return pages.filter(page => (!state || page.state === state) && (!role || page.role === role) && (!audience || page.audiences.includes(audience))).map(page => {
    const title = page.title.toLowerCase(), text = page.text.toLowerCase();
    if (!words.every(word => title.includes(word) || text.includes(word))) return null;
    const taskRole = ['Guide', 'Tutorial', 'Reference', 'Explanation'].includes(page.role);
    const ownerMatch = words.some(word => page.requirements.some(id => id.toLowerCase() === word));
    const readerScope = audience === 'Users' || audience === 'Authors';
    const readerPriority = ownerMatch ? 2 : words.length && readerScope && taskRole ? 1 : 0;
    const taskBoost = words.length && taskRole ? (page.readerView ? 2 : 12) : 0;
    const score = taskBoost + words.reduce((n, word) => n + (title.includes(word) ? 8 : 0) + (page.requirements.some(id => id.toLowerCase() === word) ? 40 : 0), 0);
    const at = words.length ? Math.max(0, text.indexOf(words[0]) - 70) : 0;
    const requirement = words.map(word => word.toUpperCase()).find(id => page.requirementAnchors[id]);
    const url = requirement ? page.url + '#' + page.requirementAnchors[requirement] : page.url;
    return {...page, url, score, readerPriority, snippet: (at ? '…' : '') + page.text.slice(at, at + 260) + (page.text.length > at + 260 ? '…' : '')};
  }).filter(Boolean).sort((a, b) => b.readerPriority - a.readerPriority || b.score - a.score || a.title.localeCompare(b.title));
}
