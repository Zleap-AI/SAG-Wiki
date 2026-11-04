// Adapted from TextFold ladder.py: assemble/local/topic/Chinese background rules.
// Copyright 2026 Xia Chen, Apache-2.0. See src-tauri/licenses/TEXTFOLD.txt.
use super::*;
use std::sync::LazyLock;
const ROLES:[&str;8]=["context","gap","claim","method","setup","result","analysis","limitation"];
const CLAIM:[&str;19]=["认为","主张","应当","应该","需要","必须","旨在","在于","而是","可见","因此","所以","证明","表明","说明","标志","意味着","构成","确立"];
const CONDITIONAL:[&str;12]=["而如果","如果","若","倘若","假如","尽管","虽然","由于","随着","鉴于","但是","即使"];
const FILLER:[&str;30]=["随着","根据","由于","因此","但是","而且","并且","不过","然而","所以","如果","虽然","尽管","目前","首先","其次","再次","最后","此外","另外","同时","综上","总之","基于","鉴于","针对","对于","关于","近年来","当前"];
const MARKERS:&str="①②③④⑤⑥⑦⑧⑨⑩⑴⑵⑶⑷⑸⑹⑺⑻⑼⑽⒈⒉⒊⒋⒌⒍⒎⒏⒐⒑⁰¹²³⁴⁵⁶⁷⁸⁹†‡§¶";
static APPARATUS:LazyLock<regex::Regex>=LazyLock::new(||regex::Regex::new(r"[.．…·]\s*[.．…·]\s*[.．…·]|^\s*[①-⑩].*(参见|前引|同前注|出版社|学报|https?://|\[[A-Za-z/]+\])").unwrap());
static ISSUE:LazyLock<regex::Regex>=LazyLock::new(||regex::Regex::new(r"^\s*(《[^》]{2,40}》|(19|20)\d{2}\s*年)").unwrap());
static DASH:LazyLock<regex::Regex>=LazyLock::new(||regex::Regex::new(r"\s*[—–]\s*|\s+-\s+").unwrap());
pub(super) fn is_cjk(text:&str)->bool {
    let mut total=0;let mut chinese=0;
    for c in text.chars().take(4000).filter(|c|c.is_alphanumeric()) {total+=1;if matches!(c,'\u{3400}'..='\u{9fff}'|'\u{3040}'..='\u{30ff}'){chinese+=1;}}
    total>0 && chinese as f64/total as f64>0.2
}
fn clean(text:&str)->String {
    let text=text.split_whitespace().collect::<Vec<_>>().join(" ").replace("——","，");
    DASH.replace_all(&text,", ").trim_end_matches(|c:char|c.is_whitespace()||".;,!?:。；，！？：、".contains(c)).to_string()
}
fn head(text:&str,budget:usize,cjk:bool)->String {
    if !cjk {return text.split_whitespace().take(budget).collect::<Vec<_>>().join(" ");}
    let chars:Vec<_>=text.chars().collect();
    if chars.len()<=budget{return text.into();}
    let edges:Vec<_>=chars.iter().enumerate().skip(2).take(budget+budget/2-2).filter(|(_,c)|"。！？；，、：,".contains(**c)).map(|(i,_)|i).collect();
    let edge=edges.iter().filter(|&&i|i<=budget).last().or_else(||edges.first()).copied();
    let end=edge.map(|i|i+usize::from("。！？；".contains(chars[i]))).unwrap_or(budget);
    chars[..end].iter().collect()
}
fn topic(text:&str,take:&str,cjk:bool)->String {
    let text=clean(text);let budget=if cjk{8}else{3};
    if !text.is_empty()&&(take.contains(&text)||if cjk{text.chars().count()<=budget+2}else{text.split_whitespace().count()<=budget+2}) {return text;}
    if !cjk{return head(take,budget,false);}
    let mut start=take.trim_start_matches(|c:char|c.is_whitespace()||MARKERS.contains(c));
    for _ in 0..2 {if let Some(prefix)=FILLER.iter().find(|prefix|start.starts_with(**prefix)){start=start[prefix.len()..].trim_start_matches(|c:char|"，、： ".contains(c));}}
    let mut text:String=start.chars().take_while(|c|!"。！？；，、：,.!?;:".contains(*c)).take(budget).collect();
    while text.ends_with(|c:char|"的地得了着与和及或主是支第被把将能可各每".contains(c)) {text.pop();}
    text
}
fn local(unit:&FoldUnit,cjk:bool)->FoldedUnit {
    let full=unit.sentences.join(" ");
    let suppressed=cjk&&APPARATUS.is_match(&full);
    let index=if cjk {unit.sentences.iter().take(4).enumerate().max_by(|(ia,a),(ib,b)|{
        let score=|i:usize,s:&str|{let mut n=4-6*i as i32;if s.chars().count()<10{return -990;}if CLAIM.iter().any(|v|s.contains(v)){n+=30;}if s.contains(['：',':']){n+=15;}if CONDITIONAL.iter().any(|v|s.starts_with(v)){n-=25;}n};
        score(*ia,a).cmp(&score(*ib,b)).then(ib.cmp(ia))
    }).map(|(i,_)|i).unwrap_or(0)}else{0};
    let key=unit.sentences[index].clone();
    let mut takeaway=head(&key,if cjk{40}else{14},cjk);
    if cjk&&CONDITIONAL.iter().any(|v|takeaway.starts_with(v))&&takeaway.chars().count()<key.chars().count(){takeaway=key.clone();}
    let label=topic("",&takeaway,cjk);
    FoldedUnit{id:unit.id,evidence:if suppressed{vec![]}else{vec![index]},key:if suppressed{String::new()}else{key},takeaway:if suppressed{String::new()}else{takeaway},topic:if suppressed{String::new()}else{label},role:"context".into(),local:true,suppressed,hide_takeaway:false,glue:String::new()}
}
pub(super) fn assemble(unit:&FoldUnit,reply:Option<&UnitReply>,cjk:bool)->FoldedUnit {
    if cjk&&APPARATUS.is_match(&unit.sentences.join(" ")){return local(unit,cjk);}
    let Some(reply)=reply else{return local(unit,cjk)};
    let take=clean(&reply.takeaway);if take.is_empty(){return local(unit,cjk);}
    let reason=clean(&reply.reason);let punctuation=if cjk{"。"}else{"."};
    let key=if reason.is_empty(){format!("{take}{punctuation}")}else{format!("{take}{punctuation}{}{reason}{punctuation}",if cjk{""}else{" "})};
    let mut evidence:Vec<_>=reply.evidence.iter().copied().filter(|&i|i<unit.sentences.len()).collect();evidence.sort_unstable();evidence.dedup();evidence.truncate(3);if evidence.is_empty(){evidence.push(0);}
    FoldedUnit{id:unit.id,evidence,key,topic:topic(&reply.topic,&take,cjk),takeaway:take,role:if ROLES.contains(&reply.role.as_str()){reply.role.clone()}else{"context".into()},local:false,suppressed:false,hide_takeaway:false,glue:String::new()}
}
pub(super) fn section(id:usize,reply:&SectionReply,cjk:bool)->FoldedSection {
    let takeaway=clean(&reply.takeaway);let topic=topic(&reply.topic,&takeaway,cjk);
    FoldedSection{id,takeaway,topic}
}
fn matches(a:&[char],b:&[char])->usize {
    let (mut best,mut ai,mut bi)=(0,0,0);
    for i in 0..a.len(){for j in 0..b.len(){let mut n=0;while i+n<a.len()&&j+n<b.len()&&a[i+n]==b[j+n]{n+=1;}if n>best{best=n;ai=i;bi=j;}}}
    if best==0{0}else{best+matches(&a[..ai],&b[..bi])+matches(&a[ai+best..],&b[bi+best..])}
}
pub(super) fn fold_background(document:&FoldDocument,result:&mut FoldResult,cjk:bool) {
    if !cjk{return;}
    for section in &document.sections {
        let summary=result.sections.iter().find(|s|s.id==section.id).map(|s|s.takeaway.as_str()).unwrap_or("");
        let flatten=|s:&str|s.chars().filter(|c|c.is_alphanumeric()).collect::<Vec<_>>();let target=flatten(summary);
        let mut run=0;let mut first=true;
        for source in &section.units {
            let Some(unit)=result.units.iter_mut().find(|u|u.id==source.id)else{continue};
            if unit.suppressed||unit.takeaway.is_empty(){run=0;continue;}
            let text=flatten(&unit.takeaway);
            if text.len()>=8&&target.len()>=8&&2.0*matches(&text,&target)as f64/(text.len()+target.len())as f64>=0.8{unit.hide_takeaway=true;continue;}
            if first||!matches!(unit.role.as_str(),"context"|"setup"){first=false;run=0;continue;}
            run+=1;
            if run>2 || (ISSUE.is_match(&unit.takeaway)&&!unit.takeaway.contains(['：',':'])&&!CLAIM.iter().any(|v|unit.takeaway.contains(v))){unit.hide_takeaway=true;}
        }
    }
}

// TextFold's dress(): carry the cut-off tail across paragraph/page boundaries.
pub(super) fn dress(document:&FoldDocument,result:&mut FoldResult,cjk:bool) {
    if !cjk{return;}
    for section in &document.sections {
        let mut previous:Option<&FoldUnit>=None;
        for source in &section.units {
            if source.kind=="caption"||source.sentences.first().is_some_and(|s|s.starts_with(|c:char|"①②③④⑤⑥⑦⑧⑨⑩".contains(c))){continue;}
            if let Some(last)=previous {
                let text=last.sentences.join(" ");let trimmed=text.trim_end_matches(|c:char|c.is_whitespace()||"①②③④⑤⑥⑦⑧⑨⑩".contains(c));
                if !trimmed.is_empty()&&!trimmed.ends_with(|c:char|"。！？.!?：:；;”’」』）)".contains(c)) {
                    let sentence=trimmed.rsplit(['。','！','？','!','?']).next().unwrap_or(trimmed);
                    let clause=sentence.rsplit(['，',',','；',';',':','：']).next().unwrap_or(sentence);
                    let tail=if clause.chars().count()>=20{clause}else{sentence};
                    let chars:Vec<_>=tail.chars().collect();let tail:String=chars[chars.len().saturating_sub(40)..].iter().collect();
                    if let Some(unit)=result.units.iter_mut().find(|u|u.id==source.id) {
                        unit.glue=tail;
                        if source.sentences.len()>1&&source.sentences[0].chars().count()<=6{unit.evidence.extend([0,1]);unit.evidence.sort_unstable();unit.evidence.dedup();unit.evidence.truncate(3);}
                    }
                }
            }
            previous=Some(source);
        }
    }
}
