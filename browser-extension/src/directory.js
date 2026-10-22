async function database() {
 return new Promise((resolve,reject)=>{
  const request=indexedDB.open('sag-web-clipper',1);
  request.onupgradeneeded=()=>request.result.createObjectStore('settings');
  request.onsuccess=()=>resolve(request.result);
  request.onerror=()=>reject(request.error);
 });
}
export async function getDirectory(destination = 'folder') {
 const db=await database();
 try {
  return await new Promise((resolve,reject)=>{
   const request=db.transaction('settings').objectStore('settings').get(destination==='default'?'default-directory':'directory');
   request.onsuccess=()=>resolve(request.result);
   request.onerror=()=>reject(request.error);
  });
 } finally {db.close();}
}
export async function setDirectory(handle, destination = 'folder') {
 const db=await database();
 try {
  await new Promise((resolve,reject)=>{
   const transaction=db.transaction('settings','readwrite');
   transaction.objectStore('settings').put(handle,destination==='default'?'default-directory':'directory');
   transaction.oncomplete=()=>resolve();transaction.onerror=()=>reject(transaction.error);
  });
 } finally {db.close();}
}
