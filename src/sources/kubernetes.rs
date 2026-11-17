//! Kubernetes: `kubectl logs --timestamps`, per un pod preciso oppure per etichetta.
//!
//! Con un'etichetta (es. `app=web`) Loglem legge tutti i pod che la portano, anche
//! quelli nuovi creati da un rilascio: i log non si interrompono quando i pod cambiano.
use crate::{archive::Archive, process, sources::podman, timestamps};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{process::Command, time::Duration};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub namespace: String,
    /// Un pod preciso…
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pod: Option<String>,
    /// …oppure i pod con questa etichetta, es. `app=web`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// Solo questo container del pod (vuoto = tutti).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// Contesto di kubectl (vuoto = quello attuale).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

fn safe(value: &str) -> bool {
    !value.is_empty() && !value.starts_with('-') && !value.chars().any(char::is_control)
}

impl Target {
    pub fn validate(&self) -> Result<()> {
        if !safe(&self.namespace) {
            bail!("Indica il namespace di Kubernetes");
        }
        match (&self.pod, &self.selector) {
            (Some(_), Some(_)) | (None, None) => {
                bail!("Indica un pod oppure un'etichetta (es. app=web), non entrambi")
            }
            _ => {}
        }
        for value in [&self.pod, &self.selector, &self.container, &self.context]
            .into_iter()
            .flatten()
        {
            if !safe(value) {
                bail!("Valore non valido: {value:?}");
            }
        }
        Ok(())
    }

    pub fn describe(&self) -> String {
        let what = match (&self.pod, &self.selector) {
            (Some(pod), _) => format!("pod {pod}"),
            (_, Some(selector)) => format!("pod con etichetta {selector}"),
            _ => "?".into(),
        };
        let container = self
            .container
            .as_deref()
            .map(|c| format!(", container {c}"))
            .unwrap_or_default();
        format!("Kubernetes {}/{what}{container}", self.namespace)
    }

    fn kubectl(&self) -> Command {
        let mut command = Command::new("kubectl");
        if let Some(context) = &self.context {
            command.arg(format!("--context={context}"));
        }
        command.arg(format!("--namespace={}", self.namespace));
        command
    }
}

pub fn collect(store: &mut Archive, target: &Target, now: i64, earliest: i64) -> Result<usize> {
    let checkpoint = "kubernetes_until";
    let since = podman::resume_point(store, checkpoint, earliest)?;
    let mut command = target.kubectl();
    command.args(["logs", "--timestamps", "--tail=-1"]);
    command.arg(format!("--since-time={}", timestamps::format_ns(since)));
    match &target.container {
        Some(container) => command.arg(format!("--container={container}")),
        None => command.arg("--all-containers"),
    };
    match (&target.pod, &target.selector) {
        (Some(pod), _) => command.arg(pod),
        (_, Some(selector)) => command.args([
            format!("--selector={selector}"),
            "--prefix".into(),
            "--max-log-requests=20".into(),
        ]),
        _ => bail!("Indica un pod oppure un'etichetta"),
    };
    let snapshot = podman::read_snapshot(store, &mut command, "kubernetes", now, earliest)?;
    if let (Some(pod), true) = (&target.pod, snapshot.diagnostic.contains("NotFound")) {
        // Le righe scritte dal vecchio pod dopo l'ultima lettura non sono più disponibili.
        let message = format!(
            "Il pod {pod} non esiste più (di solito lo ricrea un rilascio): le righe scritte \
             dopo l'ultima lettura sono perse. Per non perdere righe ai rilasci, segui \
             l'applicazione per etichetta (es. app=web) invece del singolo pod"
        );
        store.record_gap(since, now, &message)?;
        bail!("{message}");
    }
    if !snapshot.status.success() || snapshot.invalid != 0 {
        bail!(
            "Lettura Kubernetes non completata (uscita: {}; righe non riconosciute: {}). {} \
             I log già salvati restano disponibili",
            snapshot.status,
            snapshot.invalid,
            snapshot.diagnostic.trim_end()
        );
    }
    store.set_state(checkpoint, &now.to_string())?;
    Ok(snapshot.added)
}

/// Un pod trovato nel cluster, con l'etichetta `app` se c'è (per seguirlo nei rilasci).
#[derive(Debug, Clone, Serialize)]
pub struct Pod {
    pub namespace: String,
    pub name: String,
    pub containers: Vec<String>,
    pub app: Option<String>,
    pub phase: String,
}

pub fn discover(context: Option<&str>) -> Result<Vec<Pod>> {
    let mut command = Command::new("kubectl");
    if let Some(context) = context {
        command.arg(format!("--context={context}"));
    }
    command.args(["get", "pods", "--all-namespaces", "--output=json"]);
    let data = process::output(&mut command, Duration::from_secs(20))
        .context("kubectl non risponde: è installato e collegato a un cluster?")?;
    let list: serde_json::Value = serde_json::from_slice(&data)?;
    let pods = list["items"].as_array().cloned().unwrap_or_default();
    Ok(pods
        .iter()
        .map(|pod| {
            let text = |v: &serde_json::Value| v.as_str().unwrap_or_default().to_owned();
            Pod {
                namespace: text(&pod["metadata"]["namespace"]),
                name: text(&pod["metadata"]["name"]),
                containers: pod["spec"]["containers"]
                    .as_array()
                    .map(|cs| cs.iter().map(|c| text(&c["name"])).collect())
                    .unwrap_or_default(),
                app: pod["metadata"]["labels"]["app"].as_str().map(str::to_owned),
                phase: text(&pod["status"]["phase"]),
            }
        })
        .collect())
}
