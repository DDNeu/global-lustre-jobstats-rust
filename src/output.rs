//! Output formatting for glljobstat (YAML-like output)

use chrono::{Local, TimeZone};
use std::collections::HashMap;

use crate::args::Args;
use crate::op_keys::{MISC_KEYS, OP_KEYS, OP_KEYS_REV};
use crate::persistence::TopDb;

/// Format timestamp for output
pub fn format_timestamp(timestamp: i64, human_readable: bool) -> String {
    if human_readable {
        Local
            .timestamp_opt(timestamp, 0)
            .single()
            .map(|dt| dt.format("%a %d %b %Y %H-%M-%S %z").to_string())
            .unwrap_or_else(|| timestamp.to_string())
    } else {
        timestamp.to_string()
    }
}

/// Job data with operations
#[derive(Debug, Clone)]
pub struct JobOutput {
    pub job_id: String,
    pub ops: HashMap<String, i64>,
    pub sampling_window: Option<i64>,
}

/// Print a single job in YAML-like format
pub fn print_job(job: &JobOutput, args: &Args, jobid_length: usize) {
    let padded_id = format!("{}:", job.job_id);
    print!("- {:<width$} {{", padded_id, width = jobid_length);
    
    let mut first = true;
    for (short, long) in OP_KEYS.iter() {
        if *short == "rb" || *short == "wb" {
            // Skip histogram operations for now (they have special handling)
            continue;
        }
        
        if let Some(&val) = job.ops.get(*long) {
            if !first {
                print!(", ");
            }
            
            let op_name = if args.fullname { *long } else { *short };
            print!("{}: {}", op_name, val);
            first = false;
        }
    }
    
    if let Some(sw) = job.sampling_window {
        let sw_name = if args.fullname {
            MISC_KEYS.get("sw").unwrap_or(&"sw")
        } else {
            "sw"
        };
        print!(", {}: {}", sw_name, sw);
    }
    
    println!("}}");
}

/// Print top jobs header and list
pub fn print_top_jobs(
    top_jobs: &[JobOutput],
    total_jobs: usize,
    count: usize,
    query_time: i64,
    query_duration: i64,
    servers_count: usize,
    osts_count: usize,
    mdts_count: usize,
    args: &Args,
    jobid_length: usize,
) {
    let times = format_timestamp(query_time, args.humantime);
    
    println!("---"); // YAML document start
    println!("timestamp: {}", times);
    
    if args.rate || args.difference {
        println!("query_duration: {}", query_duration);
    }
    
    println!("servers_queried: {}", servers_count);
    println!("osts_queried: {}", osts_count);
    println!("mdts_queried: {}", mdts_count);
    println!("total_jobs: {}", total_jobs);
    
    let label = if args.percent {
        format!("top_{}_job_operations_in_percent_to_total_operations:", count)
    } else if args.rate {
        format!("top_{}_job_operation_rates_during_query_windows:", count)
    } else if args.difference {
        format!("top_{}_job_operation_difference_between_query_windows:", count)
    } else {
        format!("top_{}_jobs:", count)
    };
    
    print!("{}", label);
    
    if top_jobs.is_empty() {
        println!(" []");
    } else {
        println!();
        for job in top_jobs {
            print_job(job, args, jobid_length);
        }
    }
    
    if !(args.total || args.totalrate || args.percent) {
        println!("..."); // YAML document end
    }
}

/// Print total operations
pub fn print_total_ops(total_ops: &HashMap<String, i64>, args: &Args) {
    if args.rate {
        println!("total_rate_per_operation_during_query_window:");
    } else {
        println!("total_operations:");
    }
    
    // Sort by value descending
    let mut sorted: Vec<_> = total_ops.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    
    for (key, value) in sorted {
        let op_name = if args.fullname {
            key.as_str()
        } else {
            OP_KEYS_REV.get(key.as_str()).unwrap_or(&key.as_str())
        };
        println!("- {:<10} {{rate: {}}}", format!("{}:", op_name), value);
    }
    
    if !args.totalrate {
        println!("..."); // YAML document end
    }
}

/// Print highest rates ever logged
pub fn print_total_ops_logged(top_db: &TopDb, args: &Args) {
    println!("highest_rate_per_operation_in_logfile:");
    
    // Sort by rate descending
    let mut sorted: Vec<_> = top_db.top_ops.iter().collect();
    sorted.sort_by(|a, b| b.1.rate.cmp(&a.1.rate));
    
    for (key, entry) in sorted {
        let op_name = if args.fullname {
            key.as_str()
        } else {
            OP_KEYS_REV.get(key.as_str()).unwrap_or(&key.as_str())
        };
        let ts_name = if args.fullname { "timestamp" } else { "ts" };
        let times = format_timestamp(entry.timestamp, args.humantime);
        println!("- {:<10} {{rate: {:<10} {}: {}}}", 
            format!("{}:", op_name), 
            format!("{},", entry.rate),
            ts_name, 
            times
        );
    }
    
    println!("job_with_hightest_rate_per_operation_in_logfile:");
    print_top_job_per_op(top_db, args);
    
    println!("..."); // YAML document end
}

fn print_top_job_per_op(top_db: &TopDb, args: &Args) {
    for (op_key, entry) in &top_db.top_job_per_op {
        if entry.job_id.is_empty() {
            continue;
        }
        
        let op_name = if args.fullname {
            op_key.as_str()
        } else {
            OP_KEYS_REV.get(op_key.as_str()).unwrap_or(&op_key.as_str())
        };
        let ts_name = if args.fullname { "timestamp" } else { "ts" };
        
        print!("- {:<10} {{", format!("{}:", op_name));
        
        let mut items: Vec<_> = entry.ops.iter().collect();
        items.sort_by(|a, b| b.1.cmp(a.1));
        
        for (i, (k, v)) in items.iter().enumerate() {
            let item_name = if args.fullname {
                k.as_str()
            } else {
                OP_KEYS_REV.get(k.as_str()).unwrap_or(&k.as_str())
            };
            
            if i > 0 {
                print!(", ");
            }
            print!("{}: {}", item_name, v);
        }
        
        let times = format_timestamp(entry.timestamp, args.humantime);
        println!(", {}: {}, job_id: {}}}", ts_name, times, entry.job_id);
    }
}

