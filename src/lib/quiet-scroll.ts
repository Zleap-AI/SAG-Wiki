/** Keep native scrolling and its hit area; only reveal the thumb while in use. */
export function quietScroll(node: HTMLElement) {
 node.classList.add('scroll-region');
 let timer: ReturnType<typeof setTimeout>;
 const scroll = () => {
  node.classList.add('is-scrolling');
  clearTimeout(timer);
  timer = setTimeout(() => node.classList.remove('is-scrolling'), 900);
 };
 node.addEventListener('scroll', scroll, {passive: true});
 return {destroy() {
  clearTimeout(timer);
  node.removeEventListener('scroll', scroll);
  node.classList.remove('scroll-region', 'is-scrolling');
 }};
}
