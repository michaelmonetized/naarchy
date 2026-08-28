use crate::services::{BatteryState, Event};
use futures_lite::StreamExt;
use std::sync::mpsc::Sender;
use zbus::proxy;

#[proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower/devices/DisplayDevice"
)]
trait DisplayDevice {
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;
}

pub async fn run(tx: Sender<Event>) -> zbus::Result<()> {
    let conn = zbus::Connection::system().await?;
    let dev = DisplayDeviceProxy::new(&conn).await?;

    let send = |tx: &Sender<Event>, p: f64, s: u32, present: bool| {
        let _ = tx.send(Event::Battery(BatteryState {
            percent: p,
            charging: matches!(s, 1 | 4), // Charging | FullyCharged(pending discharge treat as full)
            present,
        }));
    };

    {
        let (p, s, present) = (
            dev.percentage().await.unwrap_or(0.0),
            dev.state().await.unwrap_or(0),
            dev.is_present().await.unwrap_or(false),
        );
        send(&tx, p, s, present);
        // watch for changes
        let mut changes = dev.receive_percentage_changed().await;
        loop {
            if changes.next().await.is_none() {
                break;
            }
            let p = dev.percentage().await.unwrap_or(0.0);
            let s = dev.state().await.unwrap_or(0);
            let present = dev.is_present().await.unwrap_or(false);
            send(&tx, p, s, present);
        }
    }
    Ok(())
}
