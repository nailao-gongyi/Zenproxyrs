//! allowlist 模型活性探针。
//! 背景: 动态发现只按名字判 free (`ends_with("-free")`), 上游波动
//! (Model is unavailable / not supported / FreeTierError) 不会自动摘除,
//! 客户端点到即报错。本模块定期用匿名签名实调每个模型, 非连续 200 自动摘除。
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio::sync::RwLock;

use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct Verdict {
    pub alive: bool,
    pub consecutive_fails: u32,
    pub ts: i64,
    pub status: u16,
}

pub type Verdicts = Arc<RwLock<HashMap<String, Verdict>>>;

#[derive(Clone)]
pub struct ProbeConf {
    pub interval_secs: u64,
    pub fail_threshold: u32,
    pub chat_url: String,
    pub api_key: String,
    pub models: Vec<String>,
}

pub fn probe_conf_from(st: &AppState) -> Option<ProbeConf> {
    let cfg = st.config.read().unwrap();
    let interval = cfg.live_probe_interval_secs?;
    let mut models = cfg.dynamic_model_public_allowlist.clone();
    for m in [
        "deepseek-v4-flash",
        "big-pickle",
        "mimo-v2.5",
        "hy3",
        "nemotron-3-ultra",
        "nemotron-3.5-lightning",
        "ling-3.0-flash-fin",
    ] {
        if !models.iter().any(|x| x == m) {
            models.push(m.to_string());
        }
    }
    Some(ProbeConf {
        interval_secs: interval.max(30),
        fail_threshold: cfg.live_probe_fail_threshold.max(1),
        chat_url: cfg.chat_url(),
        api_key: cfg.upstream_api_key.clone(),
        models,
    })
}

pub fn spawn(app_state: Arc<AppState>) {
    let Some(conf) = probe_conf_from(&app_state) else {
        tracing::info!("live probe disabled (LIVE_PROBE_INTERVAL_SECS unset)");
        return;
    };
    tokio::spawn(async move {
        loop {
            let st = app_state.clone();
            let c = conf.clone();
            tokio::spawn(async move {
                let mut batch: Vec<(String, u16)> = Vec::new();
                for model in &c.models {
                    let status =
                        free_model_client_rs::zen::client::probe_model(&c.chat_url, &c.api_key, model).await;
                    batch.push((model.clone(), status));
                }
                let mut verdicts = st.live_probe.write().await;
                for (model, status) in batch {
                    let alive = status == 200;
                    let entry = verdicts.entry(model.clone()).or_insert(Verdict {
                        alive: true,
                        consecutive_fails: 0,
                        ts: 0,
                        status: 200,
                    });
                    if alive {
                        if !entry.alive {
                            tracing::info!(model = %model, status, "live probe: model recovered, re-exposed");
                        }
                        entry.alive = true;
                        entry.consecutive_fails = 0;
                    } else {
                        entry.consecutive_fails += 1;
                        if entry.alive && entry.consecutive_fails >= c.fail_threshold {
                            tracing::warn!(model = %model, status, fails = entry.consecutive_fails, "live probe: model down, removing from /v1/models");
                        }
                        entry.alive = entry.consecutive_fails < c.fail_threshold;
                    }
                    entry.status = status;
                    entry.ts = chrono::Utc::now().timestamp();
                }
            });
            tokio::time::sleep(Duration::from_secs(conf.interval_secs)).await;
        }
    });
}

/// 模型当前是否被探针判定为不可用
pub async fn is_dead(verdicts: &Verdicts, model: &str) -> bool {
    verdicts
        .read()
        .await
        .get(model)
        .map(|v| !v.alive)
        .unwrap_or(false)
}

pub async fn summary(verdicts: &Verdicts) -> serde_json::Value {
    let m = verdicts.read().await;
    let mut dead = Vec::new();
    let mut alive = 0;
    for (model, v) in m.iter() {
        if v.alive {
            alive += 1;
        } else {
            dead.push(json!({"model": model, "status": v.status, "fails": v.consecutive_fails}));
        }
    }
    json!({"alive": alive, "dead": dead})
}

pub async fn dead_set(verdicts: &Verdicts) -> std::collections::HashSet<String> {
    verdicts
        .read()
        .await
        .iter()
        .filter(|(_, v)| !v.alive)
        .map(|(m, _)| m.clone())
        .collect()
}
