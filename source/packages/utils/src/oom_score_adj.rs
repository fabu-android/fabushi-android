use std::{fs, io};

pub fn reset_child_oom_score_adj(pid: Option<u32>) -> io::Result<()> {
    if !cfg!(target_os = "linux") {
        return Ok(());
    }
    let Some(pid) = pid.filter(|value| *value > 0) else {
        return Ok(());
    };
    match fs::write(format!("/proc/{pid}/oom_score_adj"), b"0") {
        Ok(()) => Ok(()),
        Err(error) => {
            // Best-effort by contract: callers must never fail because procfs is unavailable.
            let _ = error;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn undefined_or_zero_pid_is_a_noop() {
        assert!(reset_child_oom_score_adj(None).is_ok());
        assert!(reset_child_oom_score_adj(Some(0)).is_ok());
    }
}
