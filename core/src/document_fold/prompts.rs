// Adapted from TextFold ladder.py (Apache-2.0), see TEXTFOLD.txt.
use super::*;
fn intro(document: &FoldDocument) -> String {
    let section=document.sections.iter().find(|s|matches!(s.title.trim().to_lowercase().as_str(),"摘要"|"abstract"));
    let section=section.or_else(||document.sections.first().filter(|s|s.title.is_empty()));
    section.map(|s|s.units.iter().flat_map(|u|&u.sentences).cloned().collect::<Vec<_>>().join(" ")).unwrap_or_default()
}
fn outline(document:&FoldDocument,section:&FoldSection)->String {
    document.sections.iter().map(|s|format!("{}{}{}", "  ".repeat(s.depth.saturating_sub(1)), if s.id==section.id{"▶ "}else{""},s.title)).collect::<Vec<_>>().join("\n")
}
fn rule(cjk:bool)->String {
    format!("For the section: takeaway, ONE sentence of at most {} {} stating what this section establishes and how it advances the document; topic, at most {} {}, copied verbatim from the takeaway. A line that only says what the section covers is not a summary.",if cjk{40}else{16},if cjk{"characters"}else{"words"},if cjk{8}else{3},if cjk{"characters"}else{"words"})
}
const CJK_RULES:&str="\nEach line is read on its own, out of order:\n- Never supply a fact the unit does not contain: no name, standard, system, number or date of your own.\n- Do not open with 该, 此, 其, 上述, this, these or it. Name the thing again.\n- Explain unfamiliar terms in eight characters or fewer at first use.\n- Keep every number the point depends on, with what it counts, in which year and for whom.\n- A process names who acts, through which channel, to whom.\n- The reason explains the mechanism or consequence, not where the claim was published.\n";
pub(super) fn unit(document:&FoldDocument,section:&FoldSection,units:&[FoldUnit],cjk:bool)->String {
    let blocks=units.iter().enumerate().map(|(i,u)|format!("[U{i}] {}\n{}",u.kind,u.sentences.iter().enumerate().map(|(i,s)|format!("  {i}: {s}")).collect::<Vec<_>>().join("\n"))).collect::<Vec<_>>().join("\n");
    let tables=section.tables.iter().take(3).map(|t|{let text=t.chars().take(1500).collect::<String>();if t.chars().count()>1500{format!("{text} …")}else{text}}).collect::<Vec<_>>().join("\n\n");
    let context=if tables.is_empty(){String::new()}else{format!("\nTables in this section, for context (not units; takeaways may use what they show):\n{tables}")};
    let fields=[("title",document.title.clone()),("abstract",intro(document)),("outline",outline(document,section)),("head",section.title.clone()),("blocks",blocks),("context",context),("one","ONE sentence in the document's own language".into()),("more","ONE more sentence".into()),("count",if cjk{"characters"}else{"words"}.into()),("takeaway_budget",if cjk{"40"}else{"14"}.into()),("reason_budget",if cjk{"60"}else{"22"}.into()),("topic_budget",if cjk{"8"}else{"3"}.into()),("rules",if cjk{CJK_RULES}else{""}.into()),("section_rule",rule(cjk))];
    let mut prompt=include_str!("unit-prompt.txt").to_string();
    // Substitute only template slots, never reinterpret braces in source text.
    let slots=regex::Regex::new(r"\{([a-z_]+)\}").unwrap();
    prompt=slots.replace_all(&prompt,|capture:&regex::Captures|fields.iter().find(|(name,_)|*name==&capture[1]).map(|(_,value)|value.clone()).unwrap_or_else(||capture[0].to_string())).into_owned();
    if cjk {prompt=prompt.replace("no em or en dashes as punctuation, use commas or colons", "use the punctuation of the document's own language");}
    prompt
}
pub(super) fn summary(document:&FoldDocument,section:&FoldSection,units:&[FoldedUnit],cjk:bool)->String {
    let takes=section.units.iter().filter_map(|u|units.iter().find(|r|r.id==u.id)).map(|u|format!("- {}",u.takeaway)).collect::<Vec<_>>().join("\n");
    format!("Document: {}\nAbstract or opening:\n{}\nOutline:\n{}\nThis section: {}\nThe takeaways of its units, in reading order:\n{}\n{}\nUse the original language. No citations. Return JSON: {{\"section\":{{\"takeaway\":\"...\",\"topic\":\"...\"}}}}",document.title,intro(document),outline(document,section),section.title,takes,rule(cjk))
}
