export const defaultFolder = 'clippings';
export function validateFolder(value) {
 const folder = value.trim();
 if(folder && folder.split('/').some(part => !part || part === '.' || part === '..' || /[<>:"\\|?*\u0000-\u001f]/.test(part))) {
  throw new Error('invalidFolder');
 }
 return folder;
}
export function makeClipping(clip) {
 let name = clip.title.replace(/[<>:"/\\|?*\u0000-\u001f]/g,'_').trim().replace(/^\.+|\.+$/g,'');
 let bytes = 0;
 name = [...name].filter(char => {bytes += new TextEncoder().encode(char).length;return bytes <= 160;}).join('') || 'Web clipping';
 const created = new Date().toISOString();
 const filename = `${created.slice(0,19).replaceAll(':','-')} - ${name} - ${crypto.randomUUID().slice(0,8)}.md`;
 const content = `---\ntitle: ${JSON.stringify(clip.title)}\nsource: ${JSON.stringify(clip.url)}\ncreated: ${created}\n---\n\n${clip.content.trim()}\n`;
 return {filename,content};
}
