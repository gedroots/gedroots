//! Desktop → device over real iroh endpoints on this machine: pairing, a first sync, a sync after
//! an edit that only downloads the changed dataset, then removal of the device.

use gedroots_sync::{DeviceKind, Event, PairCode, Receiver, Refused, Server};
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const SHARE: &str = "/trees/tree.ged";

fn dataset(name: &str) -> Vec<u8> {
    format!(
        "0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 NAME {name} /Martin/\n1 OBJE @O1@\n0 @O1@ OBJE\n1 FILE Photos/a.jpg\n2 FORM image/jpeg\n0 TRLR\n"
    )
    .into_bytes()
}

fn refused(e: &anyhow::Error) -> Option<Refused> {
    e.downcast_ref::<Refused>().cloned()
}

#[tokio::test(flavor = "multi_thread")]
async fn pair_sync_edit_revoke() {
    let tmp = std::env::temp_dir().join(format!("gedroots-sync-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    let photo_path = tmp.join("a.jpg");
    let photo = vec![7u8; 300_000];
    fs::write(&photo_path, &photo).unwrap();
    let files = || vec![("Photos/a.jpg".to_string(), photo_path.clone())];

    let events: Arc<Mutex<Vec<Event>>> = Arc::default();
    let seen = events.clone();
    let server = Server::start(&tmp.join("desk"), move |e| seen.lock().unwrap().push(e)).await.unwrap();
    let device = Receiver::open(&tmp.join("device")).await.unwrap();

    // Not paired yet: a code that was never issued is refused.
    let code = server.pair_code(SHARE, Duration::from_secs(600)).await;
    let forged = PairCode { token: [0; 16], ..code.clone() };
    assert!(device.pair(&forged, "Phone", DeviceKind::Phone).await.is_err());

    let code = PairCode::decode(&code.encode()).unwrap();
    let desktop = device.pair(&code, "Test phone", DeviceKind::Phone).await.unwrap();
    let paired = events.lock().unwrap().clone();
    let Some(Event::Paired { device: id, share, name, kind }) = paired.first().cloned() else {
        panic!("no pairing event: {paired:?}")
    };
    assert_eq!((share.as_str(), name.as_str(), kind), (SHARE, "Test phone", DeviceKind::Phone));
    // The code is spent.
    assert!(device.pair(&code, "Test phone", DeviceKind::Phone).await.is_err());

    // Paired, but nothing published yet.
    let e = device.sync(&desktop).await.unwrap_err();
    assert_eq!(refused(&e), Some(Refused::NotShared));

    let published = server.publish(SHARE, dataset("Jean"), files()).await.unwrap();
    assert_eq!(published.files, 1);
    let seen = std::sync::Mutex::new(Vec::new());
    let first = device.sync_with(&desktop, |got, total| seen.lock().unwrap().push((got, total))).await.unwrap();
    assert_eq!(first.files, 2);
    // Progress only grows, and ends with the whole tree.
    let seen = seen.into_inner().unwrap();
    assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0), "{seen:?}");
    assert!(seen.last().is_some_and(|&(got, total)| got == total && total > 0), "{seen:?}");
    assert!(first.downloaded_bytes >= photo.len() as u64, "{first:?}");
    assert_eq!(fs::read(first.tree.join("Photos/a.jpg")).unwrap(), photo);
    assert!(fs::read_to_string(first.tree.join("gedcom.ged")).unwrap().contains("Jean"));

    server.publish(SHARE, dataset("Pierre"), files()).await.unwrap();
    let second = device.sync(&desktop).await.unwrap();
    assert!(second.downloaded_bytes < 10_000, "{second:?}");
    assert!(fs::read_to_string(second.tree.join("gedcom.ged")).unwrap().contains("Pierre"));
    assert!(events.lock().unwrap().iter().any(|e| matches!(e, Event::Synced { device: d, .. } if *d == id)));

    // Sharing off, then on again.
    server.unpublish(SHARE).await;
    assert_eq!(refused(&device.sync(&desktop).await.unwrap_err()), Some(Refused::NotShared));
    server.publish(SHARE, dataset("Pierre"), files()).await.unwrap();
    device.sync(&desktop).await.unwrap();

    // Removed: cut off at once; allowed again (next launch): accepted.
    server.revoke(&id);
    assert_eq!(refused(&device.sync(&desktop).await.unwrap_err()), Some(Refused::UnknownDevice));
    server.allow(&id, SHARE).unwrap();
    device.sync(&desktop).await.unwrap();

    // Another device, never paired.
    let stranger = Receiver::open(&tmp.join("stranger")).await.unwrap();
    assert_eq!(refused(&stranger.sync(&desktop).await.unwrap_err()), Some(Refused::UnknownDevice));

    stranger.close().await;
    device.close().await;
    server.shutdown().await.unwrap();
    let _ = fs::remove_dir_all(&tmp);
}
