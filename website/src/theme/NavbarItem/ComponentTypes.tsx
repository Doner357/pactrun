import React from 'react';
import ComponentTypes from '@theme-original/NavbarItem/ComponentTypes';
import DefaultNavbarItem from '@theme/NavbarItem/DefaultNavbarItem';
import {useHistory} from '@docusaurus/router';
import {useEdition} from '../../lib/edition';

function EditionLink({target, ...props}: {target: string; label: string; mobile?: boolean; onClick?: React.MouseEventHandler<HTMLAnchorElement>}) {
  const {prefix} = useEdition();
  return <DefaultNavbarItem {...props} to={prefix + target} />;
}

function EditionPicker({mobile, onClick}: {mobile?: boolean; onClick?: () => void}) {
  const {edition, editions, base, location} = useEdition();
  const history = useHistory();
  function change(id: string) {
    const next = editions.find(item => item.id === id)!;
    const local = location.pathname.slice((base + edition.path).length).replace(/\/$/, '') || '/';
    const page = edition.pages.find(item => (base + item.url.slice(1)).replace(/\/$/, '') === location.pathname.replace(/\/$/, ''));
    const equivalent = page && next.pages.find(item => item.source === page.source);
    if (equivalent) history.push(base + equivalent.url.slice(1) + location.search + location.hash);
    else if (['/search', '/commands'].includes(local)) history.push(base + next.path + local + location.search + location.hash);
    else history.push(base + next.path + '/?missing=' + encodeURIComponent(page?.title ?? local));
    if (mobile) onClick?.();
  }
  const picker = <label className={(mobile ? 'menu__link' : 'navbar__item') + ' edition-picker'}>Version{' '}
    <select aria-label="Documentation version" value={edition.id} onChange={event => change(event.target.value)}>
      {editions.map(item => <option key={item.id} value={item.id}>{item.label}</option>)}
    </select>
  </label>;
  return mobile ? <li className="menu__list-item">{picker}</li> : picker;
}

export default {...ComponentTypes, 'custom-editionLink': EditionLink, 'custom-editionPicker': EditionPicker};
