use crate::services::Event;
use futures_lite::StreamExt;
use std::sync::mpsc::Sender;
use zbus::proxy;

#[proxy(
    interface = "org.freedesktop.portal.Settings",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait Settings {
    fn read_one(&self, namespace: &str, key: &str) -> zbus::Result<zbus::zvariant::OwnedValue>;
    #[zbus(signal)]
    fn setting_changed(
        &self,
        namespace: &str,
        key: &str,
        value: zbus::zvariant::Value<'_>,
    ) -> zbus::Result<()>;
}

pub async fn run(tx: Sender<Event>) -> zbus::Result<()> {
    let conn = zbus::Connection::session().await?;
    let settings = SettingsProxy::new(&conn).await?;

    async fn scheme_dark(s: &SettingsProxy<'_>) -> bool {
        s.read_one("org.freedesktop.appearance", "color-scheme")
            .await
            .map(|v| value_is_dark(&v))
            .unwrap_or(true)
    }

    let _ = tx.send(Event::SchemeDark(scheme_dark(&settings).await));

    let mut stream = settings.receive_setting_changed().await?;
    while let Some(sig) = stream.next().await {
        if let Ok(args) = sig.args() {
            let ns = args.namespace().to_string();
            let key = args.key().to_string();
            if ns == "org.freedesktop.appearance" && key == "color-scheme" {
                let dark = value_is_dark(args.value());
                let _ = tx.send(Event::SchemeDark(dark));
            }
        }
    }
    Ok(())
}

/// Map xdg-desktop-portal Settings `color-scheme` to dark-mode.
///
/// Portal values: 0 = no preference, 1 = prefer dark, 2 = prefer light.
/// No preference (and anything unrecognised) falls back to dark — Omarchy's
/// default. Used only when the omarchy palette cannot be read.
fn value_is_dark(v: &zbus::zvariant::Value<'_>) -> bool {
    match v {
        zbus::zvariant::Value::U32(2) => false,
        zbus::zvariant::Value::U32(1) => true,
        _ => true,
    }
}
