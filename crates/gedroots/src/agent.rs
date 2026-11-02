//! Agents IA : serveur MCP local de GEDRoots.
//!
//! Quand le réglage est activé (File › Connect an AI Agent…), l'app sert le protocole
//! MCP en HTTP sur `127.0.0.1` : un agent comme Claude Code lit le fichier ouvert (fiches,
//! recherche, lacunes), sans jamais l'écrire : ses actes trouvés deviennent des
//! propositions, relues dans la page « À relire » avant d'être enregistrées. Les personnes
//! vivantes sont masquées.
//!
//! - le serveur tourne sur un fil à part (runtime tokio) ;
//! - chaque appel d'outil demande à la boucle de l'app une copie du document affiché
//!   (copie peu coûteuse : les enregistrements sont partagés) ;
//! - un jeton est exigé dans l'en-tête `Authorization`, et les requêtes venues d'une page
//!   web (en-tête `Origin`) sont refusées.
//!
//! Le réglage et le jeton sont propres à la machine : ils vivent dans le dossier de
//! configuration de la plateforme, jamais dans le `.ged`.

pub mod propose;
mod tools;

use gedcom::Gedcom;
use gpui_kit::*;
use std::hash::{BuildHasher, Hasher};
use std::path::PathBuf;
use std::sync::Mutex;

/// Port d'écoute, fixe pour que la commande copiée dans l'agent reste valable.
pub const PORT: u16 = 47_280;

/// Adresse du serveur MCP.
pub fn url() -> String {
    format!("http://127.0.0.1:{PORT}/mcp")
}

/// Commande à coller dans un terminal pour brancher Claude Code.
pub fn claude_command(token: &str) -> String {
    format!("claude mcp add --transport http gedroots {} --header \"Authorization: Bearer {token}\"", url())
}

/// Ce que l'app demande au document affiché, depuis le fil du serveur.
pub enum Request {
    Snapshot(tokio::sync::oneshot::Sender<Option<Snapshot>>),
    /// Proposition d'acte : ajoutée à la page « À relire » de la fenêtre, ou refusée.
    Propose(Box<propose::Pending>, tokio::sync::oneshot::Sender<Result<usize, String>>),
}

/// Copie du document de la fenêtre active.
pub struct Snapshot {
    pub path: Option<PathBuf>,
    pub ged: Gedcom,
}

/// Fenêtre dont l'agent lit le fichier : la dernière activée.
#[derive(Default)]
pub struct Target(pub Option<WeakEntity<crate::workspace::Workspace>>);

impl Global for Target {}

/// Réglage enregistré.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub enabled: bool,
    pub token: String,
}

/// État du serveur, pour la fenêtre de connexion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Off,
    Running,
    /// Le port est pris ou le serveur s'est arrêté.
    Failed(String),
}

struct Running {
    cancel: tokio_util::sync::CancellationToken,
}

static SERVER: Mutex<Option<Running>> = Mutex::new(None);
static STATUS: Mutex<Status> = Mutex::new(Status::Off);
/// File vers la boucle de l'app, créée au démarrage.
static BRIDGE: Mutex<Option<async_channel::Sender<Request>>> = Mutex::new(None);

pub fn status() -> Status {
    STATUS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn set_status(s: Status) {
    *STATUS.lock().unwrap_or_else(|e| e.into_inner()) = s;
}

/// Branche la file des requêtes sur la boucle de l'app et démarre le serveur s'il est activé.
pub fn install(cx: &mut App) {
    cx.set_global(Target::default());
    let (tx, rx) = async_channel::unbounded::<Request>();
    *BRIDGE.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
    cx.spawn(async move |cx| {
        while let Ok(req) = rx.recv().await {
            match req {
                Request::Snapshot(reply) => {
                    let snap = cx.update(|cx| {
                        let ws = cx.global::<Target>().0.as_ref()?.upgrade()?;
                        ws.read(cx).agent_snapshot()
                    });
                    let _ = reply.send(snap);
                }
                Request::Propose(pending, reply) => {
                    let res = cx.update(|cx| {
                        let ws = cx.global::<Target>().0.as_ref().and_then(|w| w.upgrade());
                        match ws {
                            Some(ws) => ws.update(cx, |ws, cx| ws.add_proposal(*pending, cx)),
                            None => Err("no file is open in GEDRoots".to_string()),
                        }
                    });
                    let _ = reply.send(res);
                }
            }
        }
    })
    .detach();
    let settings = load();
    if settings.enabled {
        start(&settings.token);
    }
}

/// Note la fenêtre que l'agent doit lire.
pub fn set_target(ws: WeakEntity<crate::workspace::Workspace>, cx: &mut App) {
    cx.set_global(Target(Some(ws)));
}

/// Active ou désactive le serveur, et enregistre le choix.
pub fn set_enabled(enabled: bool) -> Settings {
    let mut s = load();
    s.enabled = enabled;
    save(&s);
    if enabled {
        start(&s.token);
    } else {
        stop();
    }
    s
}

fn start(token: &str) {
    stop();
    let Some(bridge) = BRIDGE.lock().unwrap_or_else(|e| e.into_inner()).clone() else { return };
    let cancel = tokio_util::sync::CancellationToken::new();
    *SERVER.lock().unwrap_or_else(|e| e.into_inner()) = Some(Running { cancel: cancel.clone() });
    set_status(Status::Running);
    let token = token.to_string();
    let spawned = std::thread::Builder::new().name("gedroots-mcp".into()).spawn(move || {
        let rt = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
            Ok(rt) => rt,
            Err(e) => return set_status(Status::Failed(e.to_string())),
        };
        if let Err(e) = rt.block_on(tools::serve(bridge, token, PORT, cancel.clone())) {
            // Arrêt demandé : pas une erreur.
            if !cancel.is_cancelled() {
                set_status(Status::Failed(e));
            }
        }
    });
    if let Err(e) = spawned {
        set_status(Status::Failed(e.to_string()));
    }
}

fn stop() {
    if let Some(r) = SERVER.lock().unwrap_or_else(|e| e.into_inner()).take() {
        r.cancel.cancel();
    }
    set_status(Status::Off);
}

// ----------------------------------------------------------------------
// Réglage : `<config>/GEDRoots/agents`, lignes `cle=valeur`
// ----------------------------------------------------------------------

fn settings_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("GEDRoots").join("agents"))
}

/// Réglage enregistré ; un jeton est créé (et enregistré) s'il n'y en a pas.
pub fn load() -> Settings {
    let text = settings_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let mut s = parse(&text);
    if s.token.is_empty() {
        s.token = new_token();
        save(&s);
    }
    s
}

fn parse(text: &str) -> Settings {
    let mut s = Settings { enabled: false, token: String::new() };
    for line in text.lines() {
        match line.split_once('=') {
            Some(("enabled", v)) => s.enabled = v.trim() == "1",
            Some(("token", v)) => s.token = v.trim().to_string(),
            _ => {}
        }
    }
    s
}

fn save(s: &Settings) {
    let Some(path) = settings_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, format!("enabled={}\ntoken={}\n", u8::from(s.enabled), s.token));
}

/// Jeton aléatoire de 128 bits (clés aléatoires du système, via `RandomState`).
fn new_token() -> String {
    (0..2)
        .map(|i| {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(i);
            format!("{:016x}", h.finish())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Settings, new_token, parse};

    #[test]
    fn settings_round_trip() {
        let s = parse("enabled=1\ntoken=abc\n");
        assert_eq!(s, Settings { enabled: true, token: "abc".into() });
        assert!(!parse("").enabled);
        assert_eq!(new_token().len(), 32);
        assert_ne!(new_token(), new_token());
    }
}
