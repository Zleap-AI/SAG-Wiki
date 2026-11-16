import {packedTextMatches} from './text-matches';

// One exceptional grapheme can exceed an entire viewport's source budget.
// Normalize and search it once outside the UI thread.
self.onmessage=({data}:{data:{text:string;terms:string[];sourceStart:number}})=>{
 const pairs=packedTextMatches(data.text,data.terms,data.sourceStart);
 postMessage(pairs,{transfer:[pairs.buffer]});
};
