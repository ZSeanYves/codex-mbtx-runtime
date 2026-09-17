//! Incremental operator view; never reduces full history while an arm is running.
use std::{collections::BTreeMap, fs, io::{Read, Seek, SeekFrom}, path::{Path, PathBuf}};
use anyhow::Result;
use serde_json::{Value, json};

#[derive(Default)]
struct Tail { offset: u64, pending: String }
impl Tail {
    fn read(&mut self, path: &Path) -> Result<Vec<Value>> {
        let mut file=fs::File::open(path)?;
        file.seek(SeekFrom::Start(self.offset))?;
        let mut bytes=Vec::new();file.read_to_end(&mut bytes)?;self.offset+=bytes.len() as u64;
        // JSONL is UTF-8. Preserve any incomplete final codepoint and line by
        // rewinding it; observed lines are processed exactly once.
        let end=bytes.iter().rposition(|b|*b==b'\n').map_or(0,|i|i+1);
        self.offset-=(bytes.len()-end) as u64;
        self.pending.push_str(std::str::from_utf8(&bytes[..end])?);
        let result=self.pending.lines().filter_map(|s|serde_json::from_str(s).ok()).collect();
        self.pending.clear();Ok(result)
    }
}

pub(crate) async fn log(root: &Path, follow: bool) -> Result<()> {
    let manifest=crate::evidence::read_json(&root.join("run.json"))?;
    let planned=manifest["schedule"].as_array().map_or(0,Vec::len);
    let mut tails=BTreeMap::<PathBuf,Tail>::new();
    let mut finished=BTreeMap::<String,Value>::new();
    let mut steps=BTreeMap::<String,(usize,usize)>::new();
    loop {
        let journal=root.join("progress.jsonl");
        if journal.exists() { for entry in tails.entry(journal.clone()).or_default().read(&journal)? {
            if let Some(id)=entry["attempt_id"].as_str(){finished.insert(id.into(),entry);}
        } }
        let mut current=Value::Null;
        for directory in crate::report::directories(&root.join("attempts"))? {
            let assignment=crate::evidence::read_json(&directory.join("assignment.json")).unwrap_or(Value::Null);
            let id=assignment["attempt_id"].as_str().unwrap_or_default();
            if finished.contains_key(id){continue;}
            let count=steps.entry(id.into()).or_default();
            for bundle in crate::report::directories(&directory.join("trace")).unwrap_or_default(){
                let path=bundle.join("trace.jsonl");if !path.exists(){continue;}
                for event in tails.entry(path.clone()).or_default().read(&path)? {
                    if event["payload"]["type"] != "step_observed" {continue;}
                    let observation=&event["payload"]["observation"];
                    if observation["type"]=="started" {count.0+=1;}
                    if observation["type"]=="finished" && observation["outcome"]=="accepted" {count.1+=1;}
                }
            }
            current=json!({"assignment":assignment,"observed_steps_started":count.0,"observed_steps_accepted":count.1,"phase":if directory.join("outcome.json").exists(){"verification / finalization"}else if directory.join("process.json").exists(){"Codex agent loop / external request / pacing"}else{"workspace preparation"}});
        }
        let paired=manifest["schedule"].as_array().into_iter().flatten().filter(|p|finished.values().filter(|a|a["pair_id"]==p["pair_id"]&&a["comparable"]==true).count()==2).count();
        println!("{}",json!({"run_id":manifest["run_id"],"planned_pairs":planned,"finished_arms":finished.len(),"comparable_pairs":paired,"failed_arms":finished.values().filter(|a|a["status"]!="success").count(),"current":current,"remaining_arms":planned*2-finished.len().min(planned*2),"remaining_time_estimate":null,"scope":"incremental progress; final classifications are rebuilt from raw evidence"}));
        if !follow || finished.len()>=planned*2 {break;}
        #[cfg(unix)] {use std::os::fd::AsRawFd;let lock=fs::File::open(root.join("collector.lock"))?;if unsafe{libc::flock(lock.as_raw_fd(),libc::LOCK_SH|libc::LOCK_NB)}==0 {break;}}
        tokio::select!{_=tokio::time::sleep(std::time::Duration::from_secs(2))=>(),_=tokio::signal::ctrl_c()=>break}
    }
    Ok(())
}
