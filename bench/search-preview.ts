// Mirrors App.svelte focusResult: read, locate the match, render, mount.
import {mount} from 'svelte';
import {api} from '../src/lib/api';
import {documentMatchIndex} from '../src/lib/search-highlights';
import HighlightedDocument from '../src/components/HighlightedDocument.svelte';
import type {Candidate,Document} from '../src/lib/types';

export async function mountSearchPreview(target:HTMLElement,candidate:Candidate,terms:string[]){
 const doc=await api<Document>('read_document',{path:candidate.path});
 const initialMatch=await documentMatchIndex(doc.body,candidate.path,terms,candidate);
 const {renderMarkdown}=await import('../src/lib/markdown');
 const html=renderMarkdown(doc.body);
 const wrapper=document.createElement('article');wrapper.className='search-preview';target.append(wrapper);
 return mount(HighlightedDocument,{target:wrapper,props:{html,terms,path:candidate.path,initialMatch}});
}
