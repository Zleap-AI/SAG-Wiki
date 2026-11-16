import {isMarkdown} from './domain';
import type {Document} from './types';

export const richPieceLimit=24_000;
export const sourcePieceLimit=4_000;
export function isLargeMarkdown(doc:Pick<Document,'body'|'path'>){
 return doc.body.length>=100_000&&isMarkdown(doc.path);
}
