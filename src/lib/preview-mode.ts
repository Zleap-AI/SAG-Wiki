// Set localStorage 'sag.preview.legacy' to '1' to return to whole-document
// rendering for previews while the progressive reader is new.
export function legacyPreview(){
 try{return localStorage.getItem('sag.preview.legacy')==='1'}catch{return false}
}
