export function readerRole(pathname, base = '/') {
  const prefix = base.replace(/\/$/, '');
  const path = pathname.split(/[?#]/)[0];
  if (prefix && path !== prefix && !path.startsWith(prefix + '/')) return null;
  const local = path.slice(prefix.length).replace(/\/$/, '') || '/';
  if (/^\/package-authors(?:\/|$)/.test(local)) return 'Authors';
  if (/^\/(?:guides|pactrun-users)(?:\/|$)/.test(local) || local === '/introduction') return 'Users';
  return null;
}

export function allowReaderNavigation(current, target, base = '/') {
  const role = readerRole(current, base);
  return !role || target.replace(/\/$/, '') === base.replace(/\/$/, '') || readerRole(target, base) === role;
}
