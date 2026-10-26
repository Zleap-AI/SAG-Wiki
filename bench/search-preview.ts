// Mirrors App.svelte focusResult for a Markdown result.
import {mount} from 'svelte';
import {api} from '../src/lib/api';
import type {Candidate,Document} from '../src/lib/types';

export async function mountSearchPreview(target:HTMLElement,candidate:Candidate,terms:string[]){
 const doc=await api<Document>('read_document',{path:candidate.path});
 const [{markdownModel,candidateRanges},{default:Reader}]=await Promise.all([import('../src/lib/document-model'),import('../src/components/MarkdownReader.svelte')]);
 const model=markdownModel(doc);
 const wrapper=document.createElement('article');wrapper.className='search-preview';target.append(wrapper);
 return mount(Reader,{target:wrapper,props:{model,terms,target:candidateRanges(model,candidate)[0].from,path:candidate.path}});
}
