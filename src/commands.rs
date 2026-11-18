use super::*;
use crate::arguments::{checked_limit, Options};
use std::ffi::OsString;
fn filters(a: &mut Options) -> Result<crate::query::Query> {
    Ok(crate::query::Query {
        text: a.option("--text")?.unwrap_or_default(),
        ignore_case: a.flag("--ignore-case"),
        level: a.option("--level")?,
        only: Default::default(),
        ip: a.option("--ip")?,
        path: a.option("--path")?,
        status: a.option("--status")?,
    })
}
pub(crate) fn command(path: &str, raw: Vec<OsString>) -> Result<Action> {
    Ok(match path {
        "diagnostics" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Diagnostics;
            a.finish()?;
            action
        }
        "web" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Web;
            a.finish()?;
            action
        }
        "add" => {
            let a = Options::new(raw, &[], &[])?;
            a.finish()?;
            Action::Add
        }
        "site rename" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Site {
                action: SiteAction::Rename {
                    id: a.pos("site")?.context("Manca il sito")?,
                    name: a.pos("name")?.context("Manca il nuovo nome")?,
                },
            };
            a.finish()?;
            action
        }
        "site remove" => {
            let mut a = Options::new(raw, &["--with-logs"], &[])?;
            let action = Action::Site {
                action: SiteAction::Remove {
                    with_logs: a.flag("--with-logs"),
                    id: a.pos("site")?.context("Manca il sito")?,
                },
            };
            a.finish()?;
            action
        }
        "tui" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Tui;
            a.finish()?;
            action
        }
        "init" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Init;
            a.finish()?;
            action
        }
        "site add" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Site {
                action: SiteAction::Add {
                    name: a.pos("name")?.context("Manca name")?,
                },
            };
            a.finish()?;
            action
        }
        "site list" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Site {
                action: SiteAction::List,
            };
            a.finish()?;
            action
        }
        "site show" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Site {
                action: SiteAction::Show {
                    id: a.pos("id")?.context("Manca id")?,
                },
            };
            a.finish()?;
            action
        }
        "site copies" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Site {
                action: SiteAction::Copies {
                    id: a.pos("id")?.context("Manca id")?,
                },
            };
            a.finish()?;
            action
        }
        "site duplicates" => {
            let mut a = Options::new(raw, &["--external"], &[])?;
            let action = Action::Site {
                action: SiteAction::Duplicates {
                    id: a.pos("id")?.context("Manca id")?,
                    external: a.flag("--external"),
                },
            };
            a.finish()?;
            action
        }
        "source assign-site" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Source {
                action: SourceAction::AssignSite {
                    name: a.pos("name")?.context("Manca name")?,
                    site: a.pos("site")?.unwrap_or_default(),
                },
            };
            a.finish()?;
            action
        }
        "source configure" => {
            let mut a = Options::new(
                raw,
                &["--handoff-confirmed"],
                &[
                    "--label",
                    "--amount",
                    "--unit",
                    "--log-kind",
                    "--archive-dir",
                    "--completed-dir",
                    "--remove-rotated",
                    "--path",
                    "--container",
                    "--connection",
                ],
            )?;
            a.relationship("--amount", "--unit", true)?;
            a.relationship("--unit", "--amount", true)?;
            let action = Action::Source {
                action: SourceAction::Configure {
                    name: a.pos("name")?.context("Manca name")?,
                    label: a.option("--label")?,
                    amount: a.option("--amount")?,
                    unit: a.option("--unit")?,
                    log_kind: a.option("--log-kind")?,
                    archive_dir: a.option("--archive-dir")?,
                    completed_dir: a.option("--completed-dir")?,
                    handoff_confirmed: a.flag("--handoff-confirmed"),
                    remove_rotated: a.option("--remove-rotated")?,
                    path: a.option("--path")?,
                    container: a.option("--container")?,
                    connection: a.option("--connection")?,
                },
            };
            a.finish()?;
            action
        }
        "source rename" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Source {
                action: SourceAction::Rename {
                    name: a.pos("name")?.context("Manca name")?,
                    label: a.pos("label")?.context("Manca label")?,
                },
            };
            a.finish()?;
            action
        }
        "source pause" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Source {
                action: SourceAction::Pause {
                    name: a.pos("name")?.context("Manca name")?,
                },
            };
            a.finish()?;
            action
        }
        "source resume" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Source {
                action: SourceAction::Resume {
                    name: a.pos("name")?.context("Manca name")?,
                },
            };
            a.finish()?;
            action
        }
        "source detach" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Source {
                action: SourceAction::Detach {
                    name: a.pos("name")?.context("Manca name")?,
                },
            };
            a.finish()?;
            action
        }
        "source remove" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Source {
                action: SourceAction::Remove {
                    name: a.pos("name")?.context("Manca name")?,
                },
            };
            a.finish()?;
            action
        }
        "source add-folder" => {
            let mut a = Options::new(raw, &[], &["--site", "--name", "--path", "--days"])?;
            let action = Action::Source {
                action: SourceAction::AddFolder {
                    site: a.option("--site")?.unwrap_or_default(),
                    name: a.option("--name")?.context("Manca --name")?,
                    path: a.option("--path")?.context("Manca --path")?,
                    days: a.option("--days")?.unwrap_or(28),
                },
            };
            a.finish()?;
            action
        }
        "source add-file" => {
            let mut a = Options::new(
                raw,
                &[],
                &["--site", "--name", "--path", "--days", "--rotated-pattern"],
            )?;
            let action = Action::Source {
                action: SourceAction::AddFile {
                    site: a.option("--site")?.unwrap_or_default(),
                    name: a.option("--name")?.context("Manca --name")?,
                    path: a.option("--path")?.context("Manca --path")?,
                    days: a.option("--days")?.unwrap_or(28),
                    rotated_pattern: a.option("--rotated-pattern")?,
                },
            };
            a.finish()?;
            action
        }
        "source add-journal" => {
            let mut a = Options::new(
                raw,
                &[],
                &[
                    "--site",
                    "--name",
                    "--unit",
                    "--identifier",
                    "--match",
                    "--container",
                    "--priority",
                    "--days",
                ],
            )?;
            let action = Action::Source {
                action: SourceAction::AddJournal {
                    site: a.option("--site")?.unwrap_or_default(),
                    name: a.option("--name")?.context("Manca --name")?,
                    units: a.many("--unit")?,
                    identifiers: a.many("--identifier")?,
                    matches: a.many("--match")?,
                    container: a.option("--container")?,
                    priority: a.option("--priority")?,
                    days: a.option("--days")?.unwrap_or(28),
                },
            };
            a.finish()?;
            action
        }
        "source add-caddy" => {
            let mut a = Options::new(raw, &[], &["--site", "--name", "--path", "--days"])?;
            let action = Action::Source {
                action: SourceAction::AddCaddy {
                    site: a.option("--site")?.unwrap_or_default(),
                    name: a.option("--name")?.context("Manca --name")?,
                    path: a.option("--path")?.context("Manca --path")?,
                    days: a.option("--days")?.unwrap_or(28),
                },
            };
            a.finish()?;
            action
        }
        "source add-docker" => {
            let mut a = Options::new(
                raw,
                &[],
                &["--site", "--name", "--container", "--context", "--days"],
            )?;
            let action = Action::Source {
                action: SourceAction::AddDocker {
                    site: a.option("--site")?.unwrap_or_default(),
                    name: a.option("--name")?.context("Manca --name")?,
                    container: a.option("--container")?.context("Manca --container")?,
                    context: a.option("--context")?,
                    days: a.option("--days")?.unwrap_or(28),
                },
            };
            a.finish()?;
            action
        }
        "source add-kubernetes" => {
            let mut a = Options::new(
                raw,
                &[],
                &[
                    "--site",
                    "--name",
                    "--namespace",
                    "--pod",
                    "--selector",
                    "--container",
                    "--context",
                    "--days",
                ],
            )?;
            let action = Action::Source {
                action: SourceAction::AddKubernetes {
                    site: a.option("--site")?.unwrap_or_default(),
                    name: a.option("--name")?.context("Manca --name")?,
                    namespace: a.option("--namespace")?.context("Manca --namespace")?,
                    pod: a.option("--pod")?,
                    selector: a.option("--selector")?,
                    container: a.option("--container")?,
                    context: a.option("--context")?,
                    days: a.option("--days")?.unwrap_or(28),
                },
            };
            a.finish()?;
            action
        }
        "source add-podman" => {
            let mut a = Options::new(
                raw,
                &[],
                &["--site", "--name", "--container", "--days", "--connection"],
            )?;
            let action = Action::Source {
                action: SourceAction::AddPodman {
                    site: a.option("--site")?.unwrap_or_default(),
                    name: a.option("--name")?.context("Manca --name")?,
                    container: a.option("--container")?.context("Manca --container")?,
                    days: a.option("--days")?.unwrap_or(28),
                    connection: a.option("--connection")?,
                },
            };
            a.finish()?;
            action
        }
        "source list" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Source {
                action: SourceAction::List,
            };
            a.finish()?;
            action
        }
        "source set" => {
            let mut a = Options::new(raw, &[], &["--days", "--enabled"])?;
            let action = Action::Source {
                action: SourceAction::Set {
                    name: a.pos("name")?.context("Manca name")?,
                    days: a.option("--days")?,
                    enabled: a.option("--enabled")?,
                },
            };
            a.finish()?;
            action
        }
        "podman-list" => {
            let mut a = Options::new(raw, &[], &["--connection"])?;
            let action = Action::PodmanList {
                connection: a.option("--connection")?,
            };
            a.finish()?;
            action
        }
        "discover" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Discover;
            a.finish()?;
            action
        }
        "docker-list" => {
            let mut a = Options::new(raw, &[], &["--context"])?;
            let action = Action::DockerList {
                context: a.option("--context")?,
            };
            a.finish()?;
            action
        }
        "kubernetes-list" => {
            let mut a = Options::new(raw, &[], &["--context"])?;
            let action = Action::KubernetesList {
                context: a.option("--context")?,
            };
            a.finish()?;
            action
        }
        "status" => {
            let mut a = Options::new(raw, &["--json"], &[])?;
            let action = Action::Status {
                json: a.flag("--json"),
            };
            a.finish()?;
            action
        }
        "collect" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Collect;
            a.finish()?;
            action
        }
        "check" => {
            let mut a = Options::new(raw, &[], &["--minutes"])?;
            let action = Action::Check {
                minutes: a
                    .option("--minutes")?
                    .unwrap_or(crate::health::DEFAULT_MAX_MINUTES),
            };
            a.finish()?;
            action
        }
        "tail" => {
            let mut a = Options::new(
                raw,
                &["--ignore-case"],
                &[
                    "--source",
                    "--text",
                    "--level",
                    "--ip",
                    "--path",
                    "--status",
                    "--server",
                    "--user",
                    "--history",
                ],
            )?;
            a.relationship("--server", "--user", true)?;
            let action = Action::Tail {
                source: a.option("--source")?,
                query: filters(&mut a)?,
                server: a.option("--server")?,
                user: a.option("--user")?,
                history: a.option("--history")?.unwrap_or(10),
            };
            a.finish()?;
            action
        }
        "backup" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Backup {
                target: a.pos("target")?.context("Manca target")?,
            };
            a.finish()?;
            action
        }
        "backup-verify" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::BackupVerify {
                target: a.pos("target")?.context("Manca target")?,
            };
            a.finish()?;
            action
        }
        "restore" => {
            let mut a = Options::new(raw, &["--replace"], &[])?;
            let action = Action::Restore {
                target: a.pos("target")?.context("Manca target")?,
                replace: a.flag("--replace"),
            };
            a.finish()?;
            action
        }
        "erase" => {
            let mut a = Options::new(
                raw,
                &["--ignore-case", "--originals", "--apply"],
                &[
                    "--source", "--text", "--level", "--ip", "--path", "--status", "--reason",
                ],
            )?;
            let action = Action::Erase {
                source: a.option("--source")?,
                query: filters(&mut a)?,
                reason: a.option("--reason")?,
                originals: a.flag("--originals"),
                apply: a.flag("--apply"),
            };
            a.finish()?;
            action
        }
        "export" => {
            let mut a = Options::new(
                raw,
                &["--ignore-case", "--json"],
                &[
                    "--source", "--text", "--level", "--ip", "--path", "--status", "--output",
                ],
            )?;
            let action = Action::Export {
                source: a.option("--source")?,
                query: filters(&mut a)?,
                output: a.option("--output")?,
                json: a.flag("--json"),
            };
            a.finish()?;
            action
        }
        "audit" => {
            let mut a = Options::new(raw, &[], &["--days"])?;
            let action = Action::Audit {
                days: a.option("--days")?.unwrap_or(30),
            };
            a.finish()?;
            action
        }
        "stats" => {
            let mut a = Options::new(raw, &["--json"], &["--days"])?;
            let action = Action::Stats {
                source: a.pos("source")?.context("Manca source")?,
                days: a.option("--days")?.unwrap_or(7),
                json: a.flag("--json"),
            };
            a.finish()?;
            action
        }
        "advice" => {
            let mut a = Options::new(raw, &["--json"], &["--apply", "--choose"])?;
            a.relationship("--apply", "--choose", false)?;
            let action = Action::Advice {
                source: a.pos("source")?.context("Manca source")?,
                apply: a.option("--apply")?,
                choose: a.option("--choose")?,
                json: a.flag("--json"),
            };
            a.finish()?;
            action
        }
        "proof enable" => {
            let mut a = Options::new(raw, &[], &["--tsa"])?;
            let action = Action::Proof {
                action: ProofAction::Enable {
                    source: a.pos("source")?.context("Manca source")?,
                    tsa: a.option("--tsa")?,
                },
            };
            a.finish()?;
            action
        }
        "proof disable" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Proof {
                action: ProofAction::Disable {
                    source: a.pos("source")?.context("Manca source")?,
                },
            };
            a.finish()?;
            action
        }
        "proof verify" => {
            let mut a = Options::new(raw, &[], &["--archive", "--chain"])?;
            a.relationship("--archive", "--chain", true)?;
            let action = Action::Proof {
                action: ProofAction::Verify {
                    source: a.pos("source")?,
                    archive: a.option("--archive")?,
                    chain: a.option("--chain")?,
                },
            };
            a.finish()?;
            action
        }
        "privacy set" => {
            let mut a = Options::new(
                raw,
                &["--no-mask"],
                &["--mask-ips-after-days", "--keep-fields"],
            )?;
            a.relationship("--no-mask", "--mask-ips-after-days", false)?;
            let action = Action::Privacy {
                action: PrivacyAction::Set {
                    source: a.pos("source")?.context("Manca source")?,
                    mask_ips_after_days: a.option("--mask-ips-after-days")?,
                    no_mask: a.flag("--no-mask"),
                    keep_fields: a.option("--keep-fields")?,
                },
            };
            a.finish()?;
            action
        }
        "privacy report" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Privacy {
                action: PrivacyAction::Report,
            };
            a.finish()?;
            action
        }
        "alerts monitor" => {
            let mut a = Options::new(raw, &["--on", "--off"], &["--every"])?;
            let (on, off) = (a.flag("--on"), a.flag("--off"));
            anyhow::ensure!(!(on && off), "Scegli --on oppure --off");
            let action = Action::Alerts {
                action: AlertsAction::Monitor {
                    enabled: if on {
                        Some(true)
                    } else if off {
                        Some(false)
                    } else {
                        None
                    },
                    minutes: a.option("--every")?,
                },
            };
            a.finish()?;
            action
        }
        "alerts email-set" => {
            let mut a = Options::new(
                raw,
                &[],
                &["--from", "--to", "--page-url", "--hourly-limit"],
            )?;
            let to: String = a.option("--to")?.context("Manca --to")?;
            let action = Action::Alerts {
                action: AlertsAction::EmailSet {
                    from: a.option("--from")?.context("Manca --from")?,
                    to: to
                        .split(',')
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty())
                        .collect(),
                    page_url: a.option("--page-url")?,
                    hourly_limit: a.option("--hourly-limit")?,
                },
            };
            a.finish()?;
            action
        }
        "alerts email-test" | "alerts email-off" | "alerts email-show" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Alerts {
                action: match path {
                    "alerts email-test" => AlertsAction::EmailTest,
                    "alerts email-off" => AlertsAction::EmailOff,
                    _ => AlertsAction::EmailShow,
                },
            };
            a.finish()?;
            action
        }
        "alerts list" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Alerts {
                action: AlertsAction::List,
            };
            a.finish()?;
            action
        }
        "alerts add-pattern" => {
            let mut a = Options::new(raw, &[], &["--text", "--count", "--minutes", "--source"])?;
            let action = Action::Alerts {
                action: AlertsAction::AddPattern {
                    name: a.pos("name")?.context("Manca name")?,
                    text: a.option("--text")?.context("Manca --text")?,
                    count: a.option("--count")?.context("Manca --count")?,
                    minutes: a.option("--minutes")?.context("Manca --minutes")?,
                    source: a.option("--source")?,
                },
            };
            a.finish()?;
            action
        }
        "alerts add-size" => {
            let mut a = Options::new(raw, &[], &["--megabytes", "--source"])?;
            let action = Action::Alerts {
                action: AlertsAction::AddSize {
                    name: a.pos("name")?.context("Manca name")?,
                    megabytes: a.option("--megabytes")?.context("Manca --megabytes")?,
                    source: a.option("--source")?,
                },
            };
            a.finish()?;
            action
        }
        "alerts add-free-space" => {
            let mut a = Options::new(raw, &[], &["--megabytes"])?;
            let action = Action::Alerts {
                action: AlertsAction::AddFreeSpace {
                    name: a.pos("name")?.context("Manca name")?,
                    megabytes: a.option("--megabytes")?.context("Manca --megabytes")?,
                },
            };
            a.finish()?;
            action
        }
        "alerts remove" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Alerts {
                action: AlertsAction::Remove {
                    name: a.pos("name")?.context("Manca name")?,
                },
            };
            a.finish()?;
            action
        }
        "alerts command" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Alerts {
                action: AlertsAction::Command {
                    command: a.pos("command")?.context("Manca command")?,
                },
            };
            a.finish()?;
            action
        }
        "alerts seen" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Alerts {
                action: AlertsAction::Seen {
                    id: a.pos("id")?.context("Manca id")?,
                },
            };
            a.finish()?;
            action
        }
        "access user-add" => {
            let mut a = Options::new(raw, &[], &["--role"])?;
            let action = Action::Access {
                action: AccessAction::UserAdd {
                    name: a.pos("name")?.context("Manca name")?,
                    role: a.option("--role")?.unwrap_or(crate::access::Role::Admin),
                },
            };
            a.finish()?;
            action
        }
        "access user-remove" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Access {
                action: AccessAction::UserRemove {
                    name: a.pos("name")?.context("Manca name")?,
                },
            };
            a.finish()?;
            action
        }
        "access user-list" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Access {
                action: AccessAction::UserList,
            };
            a.finish()?;
            action
        }
        "access proxy" => {
            let mut a = Options::new(raw, &[], &["--trust", "--base-path"])?;
            let action = Action::Access {
                action: AccessAction::Proxy {
                    trust: a.many("--trust")?,
                    base_path: a.option("--base-path")?,
                },
            };
            a.finish()?;
            action
        }
        "access address" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Access {
                action: AccessAction::Address {
                    address: a.pos("address")?.context("Manca address")?,
                },
            };
            a.finish()?;
            action
        }
        "serve" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Serve;
            a.finish()?;
            action
        }
        "search" => {
            let mut a = Options::new(
                raw,
                &["--ignore-case", "--json"],
                &[
                    "--source", "--text", "--level", "--ip", "--path", "--status", "--from",
                    "--to", "--limit",
                ],
            )?;
            let action = Action::Search(SearchArgs {
                source: a.option("--source")?,
                query: filters(&mut a)?,
                from: a.option("--from")?,
                to: a.option("--to")?,
                limit: checked_limit(a.option("--limit")?.unwrap_or(100))?,
                json: a.flag("--json"),
            });
            a.finish()?;
            action
        }
        "prune" => {
            let mut a = Options::new(raw, &["--apply"], &[])?;
            let action = Action::Prune {
                apply: a.flag("--apply"),
            };
            a.finish()?;
            action
        }
        "archive-completed" => {
            let mut a = Options::new(
                raw,
                &["--producer-closed", "--remove"],
                &["--source", "--file"],
            )?;
            let action = Action::ArchiveCompleted {
                source: a.option("--source")?.context("Manca --source")?,
                file: a.option("--file")?.context("Manca --file")?,
                producer_closed: a.flag("--producer-closed"),
                remove: a.flag("--remove"),
            };
            a.finish()?;
            action
        }
        "config export" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Config {
                action: ConfigAction::Export {
                    output: a.pos("output")?.context("Manca output")?,
                },
            };
            a.finish()?;
            action
        }
        "config import" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Config {
                action: ConfigAction::Import {
                    file: a.pos("file")?.context("Manca file")?,
                },
            };
            a.finish()?;
            action
        }
        "config poll" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Config {
                action: ConfigAction::Poll {
                    seconds: a.pos("seconds")?.context("Manca seconds")?,
                },
            };
            a.finish()?;
            action
        }
        "config web-port" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::Config {
                action: ConfigAction::WebPort {
                    port: a.pos("port")?.context("Manca port")?,
                },
            };
            a.finish()?;
            action
        }
        "service install" => {
            let mut a = Options::new(raw, &["--start"], &[])?;
            let action = Action::Service {
                action: ServiceAction::Install {
                    start: a.flag("--start"),
                },
            };
            a.finish()?;
            action
        }
        "service uninstall" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Service {
                action: ServiceAction::Uninstall,
            };
            a.finish()?;
            action
        }
        "service start" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Service {
                action: ServiceAction::Start,
            };
            a.finish()?;
            action
        }
        "service stop" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Service {
                action: ServiceAction::Stop,
            };
            a.finish()?;
            action
        }
        "service status" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Service {
                action: ServiceAction::Status,
            };
            a.finish()?;
            action
        }
        "update configure" => {
            let mut a = Options::new(raw, &[], &["--repository", "--public-key"])?;
            let action = Action::Update {
                action: UpdateAction::Configure {
                    repository: a.option("--repository")?.context("Manca --repository")?,
                    public_key: a.option("--public-key")?.context("Manca --public-key")?,
                },
            };
            a.finish()?;
            action
        }
        "update check" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Update {
                action: UpdateAction::Check,
            };
            a.finish()?;
            action
        }
        "update install" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Update {
                action: UpdateAction::Install,
            };
            a.finish()?;
            action
        }
        "update status" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::Update {
                action: UpdateAction::Status,
            };
            a.finish()?;
            action
        }
        "self-check" => {
            let a = Options::new(raw, &[], &[])?;
            let action = Action::SelfCheck;
            a.finish()?;
            action
        }
        "__apply-update" => {
            let mut a = Options::new(raw, &[], &[])?;
            let action = Action::ApplyUpdate {
                stage: a.pos("stage")?.context("Manca stage")?,
            };
            a.finish()?;
            action
        }
        _ => bail!("Comando sconosciuto: {path}"),
    })
}
