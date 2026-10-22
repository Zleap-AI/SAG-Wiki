export const browserAPI = globalThis.browser ?? globalThis.chrome;
const en = {
 name:'SAG Web Clipper',save:'Save as Markdown',confirmSave:'Save',cancel:'Cancel',preview:'Markdown preview',loading:'Loading preview…',saving:'Saving…',saved:'Saved',title:'Web Clipper',
 description:'Save webpages as local Markdown files. SAG Wiki does not need to be running.',
 language:'Language',automatic:'Follow browser',directory:'Subfolder under Downloads',
 folderHelp:'Relative to your browser’s download folder. Leave empty to save directly in Downloads. Change the parent folder in browser settings, or choose a location for each save.',
 safariFolderHelp:'Relative to the system Downloads folder. Leave empty to save directly in Downloads. Saving is handled by this extension’s included app; SAG Wiki is not required.',
 apply:'Save settings',folderSaved:'Save settings updated.',downloads:'Downloads',folderSetup:'Confirm the folder when saving for the first time.',
 chooseEachTime:'Choose a location for each save',saveAs:'Choose a location for each save',
 invalidFolder:'Enter a relative folder, such as Web Clippings. Absolute paths, .. and invalid filename characters are not supported.',
 invalidPage:'Open an HTTP or HTTPS webpage to save it.',empty:'No readable page content was found.',
 savedTo:'Saved to',downloadFailed:'Download failed.',openSettings:'Settings',
 images:'Images keep their original web links.',
 invalidFilename:'The Markdown filename is invalid.',permissionDenied:'File access was denied. Choose the folder again and save your settings.',storageFull:'There is not enough space to save the file.',downloadCanceled:'Saving was canceled.',
 destination:'Save to',selectedFolder:'Custom folder',browserDownloads:'Downloads folder',chooseFolder:'Choose folder…',noFolder:'No folder selected yet',defaultDestination:'Default folder',otherFolder:'Choose another folder…',changeFolder:'Change folder…',folderIndexNotice:'After choosing another folder, add it in SAG Wiki’s folder management to search its files.',folderRequired:'Choose a save folder in extension settings and click Save settings first.',fileExists:'The file already exists. Save again to create a new filename.',folderSelected:'Save folder updated.',
};
const zh = {
 name:'SAG 网页剪藏',save:'保存为 Markdown',confirmSave:'保存',cancel:'取消',preview:'Markdown 预览',loading:'正在加载预览…',saving:'正在保存…',saved:'已保存',title:'网页剪藏',
 description:'将网页保存为本地 Markdown 文件，无需运行 SAG Wiki。',
 language:'语言',automatic:'跟随浏览器',directory:'下载目录下的子文件夹',
 folderHelp:'相对于浏览器的下载目录。留空则直接保存到下载目录。上级目录可在浏览器设置中修改，也可每次选择保存位置。',
 safariFolderHelp:'相对于系统下载目录。留空则直接保存到下载目录。由扩展安装包自带的功能保存，无需运行 SAG Wiki。',
 apply:'保存设置',folderSaved:'保存设置已更新。',downloads:'下载',folderSetup:'首次保存时，请确认文件夹位置。',
 chooseEachTime:'每次选择保存位置',saveAs:'每次选择保存位置',
 invalidFolder:'请输入相对目录，例如“网页剪辑”。不能填写绝对路径、.. 或文件名不支持的字符。',
 invalidPage:'请打开 HTTP 或 HTTPS 网页后保存。',empty:'未找到可保存的网页正文。',
 savedTo:'已保存到',downloadFailed:'下载失败。',openSettings:'设置',
 images:'图片保留原网页链接。',
 invalidFilename:'Markdown 文件名无效。',permissionDenied:'无法写入此文件夹，请重新选择文件夹并保存设置。',storageFull:'存储空间不足，无法保存文件。',downloadCanceled:'保存已取消。',
 destination:'保存到',selectedFolder:'自定义文件夹',browserDownloads:'下载目录',chooseFolder:'选择文件夹…',noFolder:'尚未选择保存文件夹',defaultDestination:'默认文件夹',otherFolder:'选择其他文件夹…',changeFolder:'更换文件夹…',folderIndexNotice:'选择其他文件夹后，请在 SAG Wiki 的文件夹管理中添加，否则无法检索其中的内容。',folderRequired:'请先在扩展设置中选择保存文件夹，并点击保存设置。',fileExists:'文件已存在，请重新保存以生成新文件名。',folderSelected:'保存文件夹已更新。',
};
export async function translator(language) {
 if(language === undefined) ({language} = await browserAPI.storage.local.get('language'));
 const locale = language || browserAPI.i18n.getUILanguage();
 const messages = locale.startsWith('zh') ? zh : en;
 return key => messages[key];
}
export async function localize(language) {
 const t = await translator(language);
 document.documentElement.lang = t('title') === zh.title ? 'zh-CN' : 'en';
 for(const element of document.querySelectorAll('[data-i18n]')) element.textContent = t(element.dataset.i18n);
 document.title = t('name');
 return t;
}

export function errorMessage(error, t) {
 const codes = {
  'folder-required':'folderRequired','invalid-folder':'invalidFolder','invalid-filename':'invalidFilename',
  'empty-content':'empty','file-exists':'fileExists','permission-denied':'permissionDenied','storage-full':'storageFull',
  NotAllowedError:'permissionDenied',SecurityError:'permissionDenied',QuotaExceededError:'storageFull',
  FILE_ACCESS_DENIED:'permissionDenied',FILE_NO_SPACE:'storageFull',USER_CANCELED:'downloadCanceled',
 };
 const key = codes[error.code] || codes[error.name] || codes[error];
 return key ? t(key) : error.message || error.error || String(error);
}
