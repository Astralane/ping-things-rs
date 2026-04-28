use dashmap::DashMap;
use solana_clock::Slot;
use solana_ledger::shred::{Shred, ShredType, Shredder};
use solana_packet::PACKET_DATA_SIZE;
use solana_sdk::signature::Signature;
use std::collections::{HashMap, HashSet};
use std::net::UdpSocket;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

#[derive(Default, Debug)]
pub struct ShredEntry {
    pub seen_slot: Option<Slot>,
}

pub type ShredMap = Arc<DashMap<Signature, ShredEntry>>;

struct SlotState {
    data_shreds: HashMap<u32, Shred>,
    deshredded_fec_sets: HashSet<u32>,
    max_data_index: u32,
    data_complete_indices: Vec<u32>,
}

impl SlotState {
    fn new() -> Self {
        Self {
            data_shreds: HashMap::new(),
            deshredded_fec_sets: HashSet::new(),
            max_data_index: 0,
            data_complete_indices: Vec::new(),
        }
    }
}

fn try_deshred(slot: Slot, state: &mut SlotState, map: &ShredMap) {
    state.data_complete_indices.sort_unstable();
    state.data_complete_indices.dedup();

    for &end_idx in &state.data_complete_indices.clone() {
        if state.deshredded_fec_sets.contains(&end_idx) {
            continue;
        }

        let start_idx = state
            .data_complete_indices
            .iter()
            .filter(|&&i| i < end_idx)
            .max()
            .map(|&i| i + 1)
            .unwrap_or(0);

        let have_all = (start_idx..=end_idx).all(|i| state.data_shreds.contains_key(&i));
        if !have_all {
            continue;
        }

        let payloads: Vec<&Shred> = (start_idx..=end_idx)
            .map(|i| &state.data_shreds[&i])
            .collect();

        match Shredder::deshred(payloads.iter().map(|s| s.payload())) {
            Ok(data) => {
                state.deshredded_fec_sets.insert(end_idx);
                match bincode::deserialize::<Vec<solana_entry::entry::Entry>>(&data) {
                    Ok(entries) => {
                        let signatures: Vec<&Signature> = entries
                            .iter()
                            .flat_map(|e| &e.transactions)
                            .flat_map(|tx| &tx.signatures)
                            .collect();
                        for sig in signatures {
                            if let Some(mut e) = map.get_mut(sig) {
                                e.seen_slot = Some(slot);
                                info!("MATCH sig={sig} slot={slot}");
                            }
                        }
                    }
                    Err(e) => {
                        debug!(
                            "deshred OK slot={slot} [{start_idx}..={end_idx}] but bincode failed: {e}"
                        );
                    }
                }
            }
            Err(e) => {
                debug!("deshred FAILED slot={slot} [{start_idx}..={end_idx}]: {e}");
            }
        }
    }
}

pub fn spawn_shred_listener(
    bind_addr: String,
    map: ShredMap,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let socket = match UdpSocket::bind(&bind_addr) {
            Ok(s) => s,
            Err(e) => {
                error!("shred listener: failed to bind {bind_addr}: {e}");
                return;
            }
        };
        if let Err(e) = socket.set_read_timeout(Some(Duration::from_secs(1))) {
            warn!("shred listener: failed to set read timeout: {e}");
        }

        info!("shred listener: listening on {bind_addr}");

        let mut buf = [0u8; PACKET_DATA_SIZE];
        let mut slots: HashMap<Slot, SlotState> = HashMap::new();

        while !cancel.is_cancelled() {
            let (size, _src) = match socket.recv_from(&mut buf) {
                Ok(r) => r,
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    continue;
                }
                Err(e) => {
                    debug!("shred listener: recv error: {e}");
                    continue;
                }
            };

            let shred = match Shred::new_from_serialized_shred(buf[..size].to_vec()) {
                Ok(s) => s,
                Err(_) => continue,
            };

            if shred.shred_type() != ShredType::Data {
                continue;
            }

            let slot = shred.slot();
            let index = shred.index();
            let is_data_complete = shred.data_complete();
            let is_last_in_slot = shred.last_in_slot();

            let state = slots.entry(slot).or_insert_with(SlotState::new);
            state.max_data_index = state.max_data_index.max(index);
            if is_data_complete || is_last_in_slot {
                state.data_complete_indices.push(index);
            }
            state.data_shreds.insert(index, shred);

            if is_data_complete || is_last_in_slot {
                try_deshred(slot, state, &map);
            }

            if slots.len() > 100 {
                let max_slot = slots.keys().max().copied().unwrap_or(0);
                slots.retain(|&s, _| s + 50 > max_slot);
            }
        }

        info!("shred listener: exiting");
    })
}
