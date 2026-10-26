export type ReaderScrollPosition={top:number;left:number;piece?:number;offset?:number};

// Session-only reading positions. Callers include the query and document hash in
// the key, and opt into restoration only when returning to the same passage.
const positions=new Map<string,ReaderScrollPosition>();
export function readReaderScroll(key:string):ReaderScrollPosition|undefined{
 const position=key?positions.get(key):undefined;
 if(position){positions.delete(key);positions.set(key,position);return {...position}}
}
export function rememberReaderScroll(key:string,position:ReaderScrollPosition){
 if(!key||!Number.isFinite(position.top)||!Number.isFinite(position.left))return;
 positions.delete(key);positions.set(key,{...position});
 while(positions.size>48)positions.delete(positions.keys().next().value!);
}
