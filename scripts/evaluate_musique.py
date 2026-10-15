#!/usr/bin/env python3
"""Prepare an isolated MuSiQue corpus and score/replay the desktop evaluation."""
import argparse
import asyncio
import collections
import hashlib
import json
import math
import re
import sqlite3
import string
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT.parent / "SAG-Benchmark" / "dataset"


def dump(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def prepare(folder):
    folder.mkdir(parents=True, exist_ok=True)
    questions_path, corpus_path = SOURCE / "musique.json", SOURCE / "musique_corpus.json"
    questions, corpus = json.loads(questions_path.read_text()), json.loads(corpus_path.read_text())
    lookup = {(d["title"], d["text"]): i for i, d in enumerate(corpus)}
    assert len(lookup) == len(corpus)
    documents = []
    for i, doc in enumerate(corpus):
        title = re.sub(r'[/\\:\x00]', "_", doc["title"]).strip()[:100] or "Untitled"
        relative = Path("corpus") / f"{i:05d}" / f"{title}.txt"
        path = folder / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        body = doc["text"].strip()
        if path.exists():
            assert path.read_text() == body
        else:
            path.write_text(body)
        documents.append({"index": i, "path": str(relative), "title": doc["title"], "sha256": digest(path)})
    selected = []
    for q in questions[:100]:
        gold = [lookup[(p["title"], p["paragraph_text"])] for p in q["paragraphs"] if p["is_supporting"]]
        assert gold and q["answerable"]
        selected.append({"id": q["id"], "question": q["question"], "answers": [q["answer"], *q["answer_aliases"]], "gold": gold})
    manifest = {"dataset": "HippoRAG 2 MuSiQue answerable validation subset", "selection": "first 100 of 1000, original order", "sources": {str(p): digest(p) for p in [questions_path, corpus_path]}, "documents": documents, "questions": selected}
    dump(folder / "manifest.json", manifest)
    print(json.dumps({"questions": len(selected), "documents": len(documents), "hops": dict(collections.Counter(q["id"].split("__")[0] for q in selected))}))


def normalize(answer):
    answer = "".join(c for c in answer.lower() if c not in string.punctuation)
    return " ".join(re.sub(r"\b(a|an|the)\b", " ", answer).split())


def f1(gold, predicted):
    a, b = normalize(gold).split(), normalize(predicted).split()
    if not a or not b:
        return float(a == b)
    common = sum((collections.Counter(a) & collections.Counter(b)).values())
    return 2 * common / (len(a) + len(b)) if common else 0.0


def extract_answer(text):
    """Read the final answer without consulting labels; preserve format deviations."""
    decoder, objects = json.JSONDecoder(), []
    for match in re.finditer(r"\{", text):
        try:
            value, end = decoder.raw_decode(text[match.start():])
        except ValueError:
            continue
        if isinstance(value, dict) and isinstance(value.get("answer"), str) and isinstance(value.get("sources"), list):
            objects.append((value, text[:match.start()].strip(), text[match.start() + end:].strip()))
    if objects:
        value, prefix, suffix = objects[-1]
        return value["answer"], value["sources"], "json", bool(prefix or suffix)
    answers = re.findall(r"(?im)^[ \t]*(?:final[ \t]+)?answer[ \t]*:[ \t]*(.+?)[ \t]*$", text)
    if answers:
        sources = re.findall(r"(?im)^[ \t]*sources?[ \t]*:[ \t]*(.*)$", text)
        ids = [int(x) for x in re.findall(r"\d+", sources[-1])] if sources and re.fullmatch(r"[\[\]\d, \t]+", sources[-1]) else []
        return answers[-1].strip().strip('*'), ids, "answer_label", True
    xml_answers = re.findall(r"<answer>\s*(.*?)\s*</answer>", text, flags=re.I | re.S)
    if xml_answers:
        return xml_answers[-1], [], "answer_tag", True
    numeric_answer = re.search(r"(?m)^[ \t]*([+-]?\d+(?:\.\d+)?)[ \t]*\s*\Z", text)
    if numeric_answer:
        return numeric_answer[1], [], "final_number", True
    raise ValueError("No final answer object or explicit Answer: line")


async def reference_replay(folder, config):
    sys.path.insert(0, str(SOURCE.parent / "scripts"))
    from sag2 import CorpusIndex, search_sag2, tokenize
    chunks = [json.loads(line) for line in (folder / "chunks.jsonl").read_text().splitlines()]
    index = CorpusIndex(folder / "reference.sqlite", [], digest(folder / "chunks.jsonl"))
    tokenizer_mismatches = []
    with index.db:
        index.db.execute("DELETE FROM chunks")
        index.db.execute("DELETE FROM chunk_fts")
        for c in chunks:
            title, body = " ".join(tokenize(c["title"])), " ".join(tokenize(c["content"]))
            if title != c["fts_title"] or body != c["fts_body"]:
                tokenizer_mismatches.append(c["id"])
            index.db.execute("INSERT INTO chunks VALUES(?,?,?,?,?,?)", (c["id"], c["document_id"], c["title"], c["content"], c["title"] + "\n" + c["content"], str(c["document_id"])))
            index.db.execute("INSERT INTO chunk_fts(rowid,title,heading,body) VALUES(?,?,?,?)", (c["id"], title, c["fts_heading"], body))
        index.db.execute("INSERT INTO chunk_fts(chunk_fts) VALUES('optimize')")
    index.total = len(chunks)
    index.frequencies = {r["term"]: r["doc"] for r in index.db.execute("SELECT term,doc FROM temp.vocab WHERE col='body'")}
    results = []
    class Replay:
        question_type = "noul"
        def __init__(self, rounds):
            self.rounds, self.position = rounds, 0
        async def score(self, query, candidates, evidence):
            r = self.rounds[self.position]
            self.position += 1
            actual = [c["id"] for c in candidates]
            if actual != r["candidate_ids"]:
                raise ValueError(f"round {self.position}: candidate order differs; expected {r['candidate_ids'][:12]}, got {actual[:12]}")
            if [c["id"] for c in evidence] != r["evidence_ids"]:
                raise ValueError(f"round {self.position}: evidence differs")
            scores = {s["id"]: s["score"] for s in r["scores"]}
            return [{**c, "jev_score": scores[c["id"]]} for c in candidates], {"requests": 0, "judgments": len(candidates), "input_tokens": 0, "output_tokens": 0, "batches": []}
    for path in sorted((folder / "results").glob("*.json")):
        row = json.loads(path.read_text())
        for n, retrieval in enumerate(row.get("retrievals", [])):
            client = Replay(retrieval["score_rounds"])
            try:
                out = await search_sag2(index, client, retrieval["query"], **{k: config["retrieval"][k] for k in ["candidate_k", "seed_k", "max_hops", "expansion_term_limit"]})
                actual = [c["id"] for c in out["candidates"][:config["retrieval"]["top_k"]]]
                expected = [c["id"] for c in retrieval["output"]["results"]]
                if actual != expected:
                    raise ValueError(f"final ranking differs: expected {expected}, got {actual}")
                if client.position != len(client.rounds):
                    raise ValueError("number of scoring rounds differs")
                steps = [step for step in out["trace"] if step["stage"] != "jev"]
                expected_steps = retrieval["output"]["trace"]
                if len(steps) != len(expected_steps):
                    raise ValueError("number of retrieval rounds differs")
                for actual_step, expected_step in zip(steps, expected_steps):
                    if actual_step["terms"] != [term["term"] for term in expected_step["terms"]]:
                        raise ValueError("expansion terms differ")
                    for actual_term, expected_term in zip(actual_step["term_scores"], expected_step["terms"]):
                        if not math.isclose(actual_term["score"], expected_term["score"], rel_tol=1e-10, abs_tol=1e-10):
                            raise ValueError("expansion term weights differ")
                results.append({"id": row["id"], "retrieval": n, "match": True})
            except (ValueError, IndexError, KeyError) as error:
                results.append({"id": row["id"], "retrieval": n, "match": False, "error": str(error)})
    index.close()
    summary = {"tokenizer_mismatch_count": len(tokenizer_mismatches), "tokenizer_mismatch_ids": tokenizer_mismatches[:100], "retrievals": len(results), "matching": sum(r["match"] for r in results), "details": results}
    dump(folder / "reference-replay.json", summary)
    return summary


def report(folder, replay=False):
    manifest, config = json.loads((folder / "manifest.json").read_text()), json.loads((folder / "config.json").read_text())
    document_lookup = {str((folder / d["path"]).resolve()): d["index"] for d in manifest["documents"]}
    rows, detailed = [], []
    for q in manifest["questions"]:
        path = folder / "results" / f"{q['id']}.json"
        if not path.exists():
            continue
        row = json.loads(path.read_text()); rows.append(row)
        prediction, parse_error = "", None
        if row["status"] == "complete":
            text = row["message"]["answer"].strip()
            try:
                prediction, sources, extraction, format_deviation = extract_answer(text)
                invalid_citations = [i for i in sources if type(i) is not int or not 1 <= i <= len(row["message"]["evidence"])]
            except (ValueError, TypeError, KeyError) as error:
                parse_error = str(error); invalid_citations = []; extraction = "unparsed"; format_deviation = True
        else:
            invalid_citations = []; extraction = "failed"; format_deviation = False
        retrieved = list(dict.fromkeys(document_lookup[c["path"]] for c in row.get("message", {}).get("evidence", [])))
        gold = set(q["gold"])
        r = {"id": q["id"], "question": q["question"], "gold_answers": q["answers"], "prediction": prediction, "status": row["status"], "error": row.get("error"), "parse_error": parse_error, "extraction":extraction, "format_deviation":format_deviation, "invalid_citations": invalid_citations, "em": max(float(normalize(a) == normalize(prediction)) for a in q["answers"]) if not parse_error and row["status"] == "complete" else 0.0, "f1": max(f1(a, prediction) for a in q["answers"]) if not parse_error and row["status"] == "complete" else 0.0, "gold_count": len(gold), "retrieved_count": len(retrieved), "tool_calls": len(row.get("message", {}).get("tool_calls", [])), "elapsed_ms": row["elapsed_ms"], "source_checks": row.get("source_checks", 0)}
        for k in [5, 10]:
            r[f"recall@{k}"] = len(gold & set(retrieved[:k])) / len(gold)
        r["recall_all"] = len(gold & set(retrieved)) / len(gold)
        r["all_gold_retrieved"] = gold.issubset(retrieved)
        r["direct_completed"] = row.get("direct_completed", False)
        direct = row["retrievals"][0] if r["direct_completed"] else None
        direct_docs = list(dict.fromkeys(document_lookup[c["path"]] for c in direct["output"]["results"])) if direct else []
        bm25 = list(dict.fromkeys(direct["first_bm25_documents"])) if direct else []
        for k in [5, 10]:
            r[f"direct_recall@{k}"] = len(gold & set(direct_docs[:k])) / len(gold)
            r[f"bm25_recall@{k}"] = len(gold & set(bm25[:k])) / len(gold)
        r["direct_all_gold_retrieved"] = gold.issubset(direct_docs)
        r["direct_elapsed_ms"] = direct["elapsed_ms"] if direct else 0
        detailed.append(r)
    n = len(detailed)
    assert n, "No results to report"
    summary = {"completed_questions": n, "requested_questions": len(manifest["questions"]), "corpus_documents": len(manifest["documents"]), "successful": sum(r["status"] == "complete" for r in detailed), "errors": sum(r["status"] != "complete" for r in detailed), "format_errors": sum(bool(r["parse_error"]) for r in detailed), "answer_em": sum(r["em"] for r in detailed) / n, "answer_f1": sum(r["f1"] for r in detailed) / n, "recall@5": sum(r["recall@5"] for r in detailed) / n, "recall@10": sum(r["recall@10"] for r in detailed) / n, "recall_all": sum(r["recall_all"] for r in detailed) / n, "all_gold_retrieved": sum(r["all_gold_retrieved"] for r in detailed), "mean_elapsed_seconds": sum(r["elapsed_ms"] for r in detailed) / n / 1000, "tool_calls": sum(r["tool_calls"] for r in detailed), "source_checks": sum(r["source_checks"] for r in detailed), "invalid_citation_count": sum(len(r["invalid_citations"]) for r in detailed), "by_hops": {}}
    for key in ["direct_recall@5", "direct_recall@10", "bm25_recall@5", "bm25_recall@10"]:
        summary[key] = sum(r[key] for r in detailed) / n
    summary["direct_all_gold_retrieved"] = sum(r["direct_all_gold_retrieved"] for r in detailed)
    summary["direct_completed"] = sum(r["direct_completed"] for r in detailed)
    summary["agent_no_tool_count"] = sum(r["tool_calls"] == 0 for r in detailed)
    summary["answer_format_deviations"] = sum(r["format_deviation"] for r in detailed)
    summary["mean_direct_retrieval_seconds"] = sum(r["direct_elapsed_ms"] for r in detailed) / n / 1000
    for hop in sorted({r["id"].split("__")[0] for r in detailed}):
        group = [r for r in detailed if r["id"].split("__")[0] == hop]
        summary["by_hops"][hop] = {"count": len(group), "em": sum(r["em"] for r in group) / len(group), "f1": sum(r["f1"] for r in group) / len(group), "recall_all": sum(r["recall_all"] for r in group) / len(group), "direct_recall@10": sum(r["direct_recall@10"] for r in group) / len(group), "bm25_recall@10": sum(r["bm25_recall@10"] for r in group) / len(group)}
    dump(folder / "summary.json", summary); dump(folder / "details.json", detailed)
    if replay:
        ref = asyncio.run(reference_replay(folder, config))
        summary["reference_replay"] = {k: v for k, v in ref.items() if k != "details"}
        dump(folder / "summary.json", summary)
    print(json.dumps(summary, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["prepare", "report"])
    parser.add_argument("folder", type=Path)
    parser.add_argument("--replay", action="store_true")
    args = parser.parse_args()
    folder = args.folder.resolve()
    prepare(folder) if args.command == "prepare" else report(folder, args.replay)
