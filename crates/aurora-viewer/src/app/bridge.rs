//! Worn bridge lifecycle and the single Movelock action used by all inputs.
use super::*;
use uuid::Uuid;

impl App {
    pub(super) fn release_bridge(&mut self) {
        self.release_native_movelock();
        if let Some(command) = self.bridge_runtime.release() {
            self.send(command);
        }
    }
    pub(super) fn toggle_move_lock(&mut self) {
        if !self.in_world() {
            return;
        }
        let prefs = self.settings.bridge.entry(self.world.agent_id).or_default();
        prefs.enabled = !prefs.enabled;
        if prefs.enabled && self.bridge_runtime.error.is_some() {
            self.bridge_runtime.retry(Instant::now());
        }
        if !self.demo {
            self.settings.save();
        }
        self.update_native_movelock();
        self.update_bridge();
    }
    pub(super) fn update_bridge(&mut self) {
        let folder_names = self.demo && matches!(self.ao_demo.as_deref(), Some("folder-names" | "folder-names-wait"));
        if self.demo && self.frame_count < 300 {
            return;
        }
        if self.demo
            && self.frame_count == 300
            && (std::env::var_os("AURORA_DEMO_MOVELOCK").is_some()
                || std::env::var_os("AURORA_DEMO_MOVEMENT").is_some()
                || std::env::var_os("AURORA_DEMO_BRIDGE_QUERY").is_some()
                || crate::ao::water_demo::mode().is_some()
                || folder_names)
        {
            crate::world::bridge::demo::seed(&mut self.world);
            if self.ao_demo.is_none() {
                crate::ao::demo::inventory_services(&mut self.world, true);
            }
            self.movelock_ui.open =
                crate::ao::water_demo::mode().is_none() && !folder_names && std::env::var_os("AURORA_DEMO_BRIDGE_QUERY").is_none();
            if std::env::var("AURORA_DEMO_BRIDGE_QUERY").is_ok_and(|m| m == "radar") {
                crate::world::bridge::demo::seed_queries(&mut self.world);
                self.panels.people = true;
                self.panels.people_tab = 0;
            }
            self.panels.perf = false;
            self.settings.bridge.entry(self.world.agent_id).or_default().enabled =
                std::env::var("AURORA_DEMO_MOVELOCK").is_ok_and(|mode| mode != "off");
            if std::env::var("AURORA_DEMO_MOVELOCK").is_ok_and(|mode| mode.starts_with("native")) {
                self.settings.bridge.entry(self.world.agent_id).or_default().mode = crate::world::bridge::LockMode::NativeSit;
                crate::ao::demo::assets(&mut self.scene);
                crate::ao::demo::native_anchor_assets(&mut self.scene);
                self.panels.chat = false;
                self.panels.inventory = false;
                self.panels.people = false;
                crate::ui::widgets::place_floater(&self.egui_ctx, "movelock", egui::pos2(60.0, 90.0), egui::vec2(470.0, 450.0));
                let selected = crate::ao::demo::seed(&mut self.world.inventory, self.world.agent_id);
                self.settings.ao.accounts.insert(
                    self.world.agent_id,
                    crate::ao::Account {
                        selected,
                        enabled: std::env::var("AURORA_DEMO_MOVELOCK").is_ok_and(|mode| mode != "native-missing"),
                        ..Default::default()
                    },
                );
            }
        }
        let online = self.in_world()
            && (!self.demo
                || std::env::var_os("AURORA_DEMO_MOVELOCK").is_some()
                || std::env::var_os("AURORA_DEMO_MOVEMENT").is_some()
                || std::env::var_os("AURORA_DEMO_BRIDGE_QUERY").is_some()
                || crate::ao::water_demo::mode().is_some()
                || folder_names);
        if !online {
            return;
        }
        self.bridge_runtime.set_enabled(self.settings.use_bridge, Instant::now());
        let prefs = self.settings.bridge.entry(self.world.agent_id).or_default();
        let effective = self.native_anchor.bridge_preferences(*prefs);
        self.bridge_runtime.update(&mut self.world, &effective, online, Instant::now());
        self.egui_ctx
            .data_mut(|d| d.insert_temp(egui::Id::unique("movelock_requested"), prefs.enabled));
        if self.demo {
            let mode = std::env::var("AURORA_DEMO_MOVELOCK").unwrap_or_default();
            self.bridge_demo.startup_stalled = mode == "timeout";
            self.bridge_demo.invalid_endpoint = mode == "endpoint";
            self.bridge_demo.halt = mode == "halted";
            let prefs = self.settings.bridge.get(&self.world.agent_id).copied().unwrap_or_default();
            self.bridge_demo.state(
                &mut self.bridge_runtime,
                mode == "moving"
                    || self.input.forward
                    || self.input.back
                    || self.input.strafe_left
                    || self.input.strafe_right
                    || self.input.up
                    || self.input.down,
                mode == "seated" || self.world.agent.is_sitting(),
                &prefs,
            );
        }
        for command in std::mem::take(&mut self.bridge_runtime.commands) {
            self.send(command);
        }
        let endpoint = self.sync_bridge_queries();
        for group in self.settings.maps.tactical.group_colors.keys() {
            if self.settings.maps.tactical.dots_enabled {
                self.world.group_data.ensure_members(*group);
            }
        }
        let group = self.world.groups.active;
        if self.settings.maps.tactical.share_group || self.settings.maps.tactical.ping_audience == 2 {
            self.world.group_data.ensure_members(group);
        }
        let allied_group = {
            let t = &self.settings.maps.tactical;
            t.available() && t.allies.enabled && t.allies.active_group && !group.is_nil()
        };
        if allied_group {
            self.world.group_data.ensure_members(group);
        }
        let group_members = if self.settings.maps.tactical.share_group || self.settings.maps.tactical.ping_audience == 2 {
            self.world.group_data.get(&group).filter(|g| g.members_complete)
        } else {
            None
        };
        // Allies (Allies tab): ticked residents, the worn group and allied group titles.
        let ally_ids = {
            let t = &self.settings.maps.tactical;
            if t.available() {
                t.allies.resolve(&self.world, &t.recipients)
            } else {
                Default::default()
            }
        };
        let allies_ready = !allied_group || self.world.group_data.get(&group).is_some_and(|g| g.members_complete);
        {
            use base64::{Engine, engine::general_purpose::STANDARD};
            let cfg = &mut self.settings.maps.tactical;
            // Expiry must also run when the minimap window is closed.
            cfg.prune();
            if !cfg.pings_enabled {
                cfg.session.stop_pings();
            }
            for (sender, payload) in std::mem::take(&mut self.bridge_runtime.tactical_events) {
                if !cfg.available() || !cfg.share || !cfg.receive_enabled || payload.len() > 640 {
                    continue;
                }
                let selected = cfg.recipients.get(&sender) == Some(&true) || group_members.is_some_and(|g| g.members.contains_key(&sender));
                // An ally may send pings (not marker sets, which only the selected can).
                let ally = ally_ids.contains(&sender);
                if !selected
                    && !ally
                    && !cfg
                        .session
                        .pings
                        .iter()
                        .any(|p| p.sender == sender || p.own() && p.recipients.contains(&sender))
                {
                    continue;
                }
                let Ok(bytes) = STANDARD.decode(&payload) else {
                    cfg.session.share_error = true;
                    continue;
                };
                if let Ok(message) = serde_json::from_slice::<crate::combat::pings::Message>(&bytes) {
                    use crate::combat::pings::Action;
                    let participant = match &message.action {
                        Action::Place { .. } => false,
                        Action::Acknowledge { id } => cfg
                            .session
                            .pings
                            .iter()
                            .any(|p| p.own() && p.marker.id == *id && p.recipients.contains(&sender)),
                        Action::Cancel { id } | Action::Contact { id } => {
                            cfg.session.pings.iter().any(|p| p.sender == sender && p.marker.id == *id)
                        }
                    };
                    if selected || ally || participant {
                        cfg.accept_ping(sender, message, Instant::now());
                    }
                } else if !selected {
                    continue;
                } else if let Ok(chunk) = serde_json::from_slice::<crate::combat::transfer::Chunk>(&bytes) {
                    cfg.session.share_error |= cfg.session.inbox.accept(sender, chunk, Instant::now()).is_err();
                } else {
                    cfg.session.share_error = true;
                }
            }
            let outgoing: Vec<_> = if cfg.available()
                && cfg.share
                && cfg.send_enabled
                && endpoint.is_some()
                && (cfg.ping_audience != 2 || group_members.is_some())
                && (cfg.ping_audience != crate::combat::allies::AUDIENCE || allies_ready)
            {
                cfg.session.outgoing.drain(..cfg.session.outgoing.len().min(1)).collect()
            } else {
                if !cfg.available() || !cfg.share || !cfg.send_enabled {
                    cfg.session.stop_sharing();
                }
                Vec::new()
            };
            let mut commands = Vec::new();
            if cfg.available()
                && cfg.share
                && cfg.send_enabled
                && let Some(endpoint) = endpoint.as_ref()
            {
                let mut payloads = Vec::new();
                for (marker, kind) in outgoing {
                    if !cfg.session.pings.iter().any(|p| p.marker.id == marker.id) {
                        continue;
                    }
                    let id = marker.id;
                    let encoded = serde_json::to_vec(&crate::combat::pings::Message::new(crate::combat::pings::Action::Place {
                        id,
                        position: marker.position,
                        kind,
                    }));
                    let Ok(bytes) = encoded else {
                        cfg.session.share_error = true;
                        continue;
                    };
                    if !marker.valid() || bytes.len() > 480 {
                        cfg.session.share_error = true;
                        continue;
                    }
                    payloads.push((bytes, true, Some(id), None));
                }
                // A response/cancel keeps its original recipients even when the
                // player subsequently changes the placement audience.
                if let Some(control) = cfg.session.ping_controls.pop_front()
                    && let Ok(bytes) = serde_json::to_vec(&control.message)
                {
                    payloads.push((bytes, true, None, Some(control.targets)));
                }
                if (!cfg.share_group || group_members.is_some())
                    && cfg
                        .session
                        .last_set_send
                        .is_none_or(|at| at.elapsed() >= Duration::from_millis(100))
                    && let Some(chunk) = cfg.session.set_outgoing.pop_front()
                {
                    cfg.session.last_set_send = Some(Instant::now());
                    payloads.push((chunk.into_bytes(), false, None, None));
                }
                for (bytes, ping, ping_id, explicit_targets) in payloads {
                    if bytes.len() <= 480 {
                        let payload = STANDARD.encode(bytes);
                        let recipients: std::collections::BTreeSet<Uuid> = cfg
                            .recipients
                            .iter()
                            .filter(|(_, on)| **on && (!ping || cfg.ping_audience == 1))
                            .map(|(id, _)| *id)
                            .chain(
                                group_members
                                    .filter(|_| if ping { cfg.ping_audience == 2 } else { cfg.share_group })
                                    .into_iter()
                                    .flat_map(|g| g.members.keys().copied()),
                            )
                            .chain(
                                ally_ids
                                    .iter()
                                    .copied()
                                    .filter(|_| ping && cfg.ping_audience == crate::combat::allies::AUDIENCE),
                            )
                            .filter(|id| *id != self.world.agent_id && !id.is_nil())
                            .collect();
                        let recipients: Vec<_> = explicit_targets.unwrap_or_else(|| recipients.into_iter().collect());
                        let invalid_recipients = recipients.is_empty() || recipients.len() > 512;
                        if let Some(id) = ping_id
                            && let Some(own) = cfg.session.pings.iter_mut().find(|p| p.own() && p.marker.id == id)
                        {
                            own.recipients = if invalid_recipients {
                                Default::default()
                            } else {
                                recipients.iter().copied().collect()
                            };
                            own.sharing = if invalid_recipients {
                                crate::combat::pings::ShareState::Failed
                            } else {
                                crate::combat::pings::ShareState::Attempted
                            };
                        }
                        if invalid_recipients {
                            cfg.session.share_error = true;
                            if !ping {
                                cfg.session.set_outgoing.clear();
                            }
                            continue;
                        }
                        let batch_size = ((700 - payload.len()) / 37).clamp(1, 16);
                        for targets in recipients.chunks(batch_size) {
                            commands.push(NetCommand::BridgeQuery {
                                request: Uuid::new_v4(),
                                endpoint: endpoint.clone(),
                                query: aurora_net::bridge::query::Query::Tactical {
                                    targets: targets.to_vec(),
                                    payload: payload.clone(),
                                },
                            });
                        }
                    }
                }
            }
            if std::mem::take(&mut cfg.session.sound_pending)
                && cfg.ping_sound
                && let Ok(id) = Uuid::parse_str(&cfg.sound_asset)
            {
                self.scene.sounds.play_ui_gain(self.audio_engine.as_ref(), id, cfg.sound_volume);
            }
            for command in commands {
                self.send(command);
            }
        }
        for ping in &self.settings.maps.tactical.session.pings {
            if !ping.own() {
                self.world.social.want_name(ping.sender);
            }
            for responder in &ping.responders {
                self.world.social.want_name(*responder);
            }
        }
        if let Some(endpoint) = endpoint.as_ref() {
            let enabled = self.settings.combat.active();
            let now = Instant::now();
            if enabled && self.combat_subscription.is_some_and(|(_, was, _)| !was) && self.chat_ui.combat.received > 0 {
                self.chat_ui.combat.awards.invalidate();
            }
            if self.combat_subscription.is_none_or(|(epoch, previous, at)| {
                epoch != endpoint.token || previous != enabled || now.duration_since(at).as_secs() >= 10
            }) {
                self.combat_subscription = Some((endpoint.token, enabled, now));
                self.send(NetCommand::BridgeQuery {
                    request: Uuid::new_v4(),
                    endpoint: endpoint.clone(),
                    query: aurora_net::bridge::query::Query::Combat(enabled),
                });
            }
        } else {
            self.combat_subscription = None;
        }
        let (command, message) = self.bridge_services.update(endpoint.as_ref(), Instant::now());
        if let Some(command) = command {
            self.send(command);
        }
        if let Some(message) = message {
            self.world.system_message(message);
        }
        if self.demo
            && self.frame_count == 700
            && std::env::var("AURORA_DEMO_BRIDGE_QUERY").is_ok_and(|m| matches!(m.as_str(), "scripts" | "object" | "error"))
        {
            let object = self
                .world
                .objects
                .iter()
                .find(|(_, o)| !o.is_avatar() && Some(o.key.region) == self.world.main_region)
                .map(|(_, o)| o.full_id);
            if let Some(object) = object {
                if std::env::var("AURORA_DEMO_BRIDGE_QUERY").is_ok_and(|m| m == "object") {
                    self.on_ctx_action(crate::ui::context::CtxAction::ObjectInfo(object));
                } else {
                    self.inspect_scripts(object);
                }
            }
        }
    }
    pub(super) fn sync_bridge_queries(&mut self) -> Option<aurora_net::bridge::Endpoint> {
        let endpoint = if self.in_world() && !self.world.teleporting {
            self.bridge_runtime.query_endpoint()
        } else {
            None
        };
        if let Some(message) = self
            .bridge_services
            .sync(self.world.agent_id, self.world.main_region, endpoint.as_ref())
        {
            self.world.system_message(message);
        }
        endpoint
    }
    pub(super) fn inspect_scripts(&mut self, object: Uuid) {
        self.inspect_bridge_information(object, false);
    }
    pub(super) fn inspect_object_information(&mut self, object: Uuid) {
        self.inspect_bridge_information(object, true);
    }
    fn inspect_bridge_information(&mut self, object: Uuid, extended: bool) {
        if !self.in_world() {
            return;
        }
        let valid = self
            .world
            .objects
            .index_of_uuid(&object)
            .and_then(|i| self.world.objects.get(i))
            .is_some_and(|o| Some(o.key.region) == self.world.main_region);
        if !valid {
            self.world.system_notice(
                if extended {
                    aurora_i18n::tr!("bridge-object-info-region")
                } else {
                    aurora_i18n::tr!("bridge-script-info-region")
                },
                crate::world::notice_topics::Topic::Items,
            );
            return;
        }
        if !self.settings.use_bridge {
            self.world
                .system_notice(aurora_i18n::tr!("bridge-disabled"), crate::world::notice_topics::Topic::Items);
            return;
        }
        let _ = self.sync_bridge_queries();
        let queued = if extended {
            self.bridge_services
                .inspect_object(object, Instant::now(), &self.world.avatar_lib.attach_names)
        } else {
            self.bridge_services.inspect(object, Instant::now())
        };
        if queued {
            if self.bridge_runtime.error.is_some() {
                self.bridge_runtime.retry(Instant::now());
            }
            self.world.system_notice(
                if extended {
                    aurora_i18n::tr!("bridge-object-info-loading")
                } else {
                    aurora_i18n::tr!("bridge-script-info-loading")
                },
                crate::world::notice_topics::Topic::Items,
            );
        }
    }
    pub(super) fn bridge_demo_command(&mut self, command: &NetCommand) -> bool {
        let mode = std::env::var("AURORA_DEMO_MOVELOCK").unwrap_or_default();
        let moving = mode == "moving"
            || self.input.forward
            || self.input.back
            || self.input.strafe_left
            || self.input.strafe_right
            || self.input.up
            || self.input.down;
        let Some(events) = self.bridge_demo.reply(
            &self.bridge_runtime,
            &self.world,
            command,
            moving,
            mode == "seated" || self.world.agent.is_sitting(),
            mode == "error",
        ) else {
            return false;
        };
        for event in events {
            let now = Instant::now();
            if let Some(event) = self.bridge_runtime.receive(&mut self.world, event, now, now) {
                self.on_app_event(event);
            }
        }
        true
    }
    pub(super) fn movelock_window(&mut self, ctx: &egui::Context, p: &Palette) {
        if std::mem::take(&mut self.toolbar_ui.movelock_options) {
            self.movelock_ui.open = true;
        }
        let prefs = self.settings.bridge.entry(self.world.agent_id).or_default();
        let fallback = prefs.enabled && prefs.mode == crate::world::bridge::LockMode::NativeSit && self.native_anchor.bridge_fallback();
        let status = if prefs.enabled && prefs.mode == crate::world::bridge::LockMode::NativeSit {
            if fallback {
                aurora_i18n::tr!("movelock-sit-needs-stand")
            } else {
                self.native_anchor.status.label()
            }
        } else {
            prefs.state()
        };
        let was_enabled = prefs.enabled;
        let shortcut = self
            .settings
            .keybinds
            .get(Action::MoveLock)
            .iter()
            .flatten()
            .map(|b| b.label())
            .collect::<Vec<_>>()
            .join(" / ");
        self.toolbar_ui.movelock_active = prefs.enabled;
        self.toolbar_ui.movelock_status = format!("{} ({shortcut})\n{status}", aurora_i18n::tr!("movelock-title"));
        if prefs.enabled && prefs.indicator {
            let label = if fallback {
                aurora_i18n::tr!("movelock-short-ao")
            } else if prefs.mode == crate::world::bridge::LockMode::NativeSit {
                self.native_anchor.status.short_label()
            } else {
                aurora_i18n::tr!("movelock-short-on")
            };
            self.movelock_ui.indicator(ctx, p, label, status);
        }
        let diagnostic = (prefs.mode == crate::world::bridge::LockMode::Bridge || fallback)
            .then(|| self.bridge_runtime.error.as_deref().or(self.bridge_runtime.progress()))
            .flatten();
        let (changed, repair) = self.movelock_ui.show(ctx, p, prefs, status, diagnostic, &shortcut);
        if repair || (prefs.enabled && !was_enabled && self.bridge_runtime.error.is_some()) {
            // Repair preserves the requested mode.
            self.bridge_runtime.retry(Instant::now());
        }
        if changed && !self.demo {
            self.settings.save();
        }
    }
}
