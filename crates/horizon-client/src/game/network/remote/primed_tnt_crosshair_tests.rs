use super::*;

#[derive(Clone, Default)]
struct ClickSpy(Arc<Mutex<Vec<&'static str>>>);

#[mod_api::__subscriptions]
impl Hooks for ClickSpy {
    fn client_packet_send(
        &self,
        _: &dyn mod_api::ClientHost,
        event: &mut mod_api::ClientPacketSendEvent<'_>,
    ) -> Flow {
        let action = match event.packet.id.path() {
            "attack" => "attack",
            "interact" => "interact",
            "use_item" => "use_item",
            "use_item_on" => "use_item_on",
            _ => return Flow::Continue,
        };
        self.0.lock().unwrap().push(action);
        Flow::Continue
    }
}

#[test]
#[ignore = "daily"]
fn the_crosshair_picks_only_visible_in_reach_primed_tnt() {
    let spy = ClickSpy::default();
    let mods = vec![mod_api::ModEntry::new(
        "tnt_pick_spy",
        Box::new(spy.clone()),
    )]
    .into();
    mod_platform::testing::with_mods(mods, || {
        with_interaction_game(true, |game| {
            game.set_mode_id(0);
            game.entity_reach = 3.0;
            game.block_reach = 4.5;
            game.yaw = 0.0;
            game.pitch = 0.0;
            game.add_primed_tnt(
                horizon_core::position::BlockPos::new(0, 65, 5),
                0.0,
                80,
                None,
                None,
            );
            let id = game.tnts[0].id as i32;
            let cow = game.res.mob_types().get("minecraft:cow").unwrap().clone();
            let stone = game
                .pack
                .assets
                .registry
                .default_state("minecraft:stone")
                .unwrap();
            let air = game.res.facts().air;
            for (case, visible) in [
                ("open", true),
                ("wall", false),
                ("nearer_cow", false),
                ("farther_cow", true),
                ("out_of_reach", false),
                ("removed", false),
            ] {
                game.world
                    .set(0, 65, 4, if case == "wall" { stone } else { air })
                    .unwrap();
                game.tick_hit.set(None);
                game.mobs.clear();
                if case.ends_with("_cow") {
                    let z = if case == "nearer_cow" { 4.5 } else { 6.5 };
                    game.mobs.push(horizon_client::mobs::Mob::new(
                        id as u32 + 1,
                        cow.clone(),
                        Vec3::new(0.5, 65.0, z),
                        0.0,
                    ));
                }
                if case == "out_of_reach" {
                    game.tnts[0].position.z = 7.5;
                } else if case == "removed" {
                    game.remove_entity(id);
                }
                let hit = game
                    .pick_entity_hit_at()
                    .is_some_and(|(_, target)| target == Some(id));
                println!("tnt-crosshair: {case} {hit}");
                assert_eq!(hit, visible, "{case}");
                if case == "open" {
                    assert!(game.entity_targeted());
                    game.remote_start_attack();
                    assert_eq!(*spy.0.lock().unwrap(), ["attack"]);
                    spy.0.lock().unwrap().clear();
                    game.world.set(0, 65, 6, stone).unwrap();
                    game.inventory[game.selected] = Some(ItemStack::new("minecraft:stick", 1));
                    game.offhand = None;
                    game.remote_use(mod_api::InteractionAttempt::Press);
                    assert_eq!(*spy.0.lock().unwrap(), ["interact", "use_item", "interact"]);
                    game.world.set(0, 65, 6, air).unwrap();
                }
            }
        });
    });
}

#[derive(Clone, Debug, PartialEq)]
enum ServerNotice {
    Attack(i32),
    InteractSpecific(i32, [f64; 3]),
    Interact(i32, Hand),
}

#[derive(Clone, Default)]
struct HookSpy(Arc<Mutex<Vec<ServerNotice>>>);

#[mod_api::__subscriptions]
impl Hooks for HookSpy {
    fn attack_entity(
        &self,
        _: &mut dyn ServerHost,
        event: &mut mod_api::AttackEntityEvent<'_>,
    ) -> Flow {
        assert_eq!(event.kind, "minecraft:tnt");
        self.0
            .lock()
            .unwrap()
            .push(ServerNotice::Attack(event.entity));
        Flow::Continue
    }

    fn entity_interact_specific(
        &self,
        _: &mut dyn ServerHost,
        event: &mut mod_api::EntityInteractSpecificEvent<'_>,
    ) -> Flow {
        assert_eq!(event.kind, "minecraft:tnt");
        self.0
            .lock()
            .unwrap()
            .push(ServerNotice::InteractSpecific(event.entity, event.location));
        Flow::Continue
    }

    fn entity_interact(
        &self,
        host: HostSide<'_>,
        event: &mut mod_api::EntityInteractEvent<'_>,
    ) -> Option<Answer> {
        assert!(matches!(host, HostSide::Server(_)));
        assert_eq!(event.kind, "minecraft:tnt");
        self.0
            .lock()
            .unwrap()
            .push(ServerNotice::Interact(event.entity, event.hand));
        None
    }
}

#[test]
#[ignore = "daily"]
fn primed_tnt_packets_reach_the_hooks_and_leave_bystanders_untouched() {
    use horizon_protocol::packets::play::{self, Serverbound as S};

    let spy = HookSpy::default();
    let mods = vec![mod_api::ModEntry::new(
        "tnt_action_spy",
        Box::new(spy.clone()),
    )]
    .into();
    mod_platform::testing::with_mods(mods, || {
        with_game(|game| {
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            let _guest = setup_interaction_guest(game);
            game.as_guest(12, |game| game.attack_ticks = 5).unwrap();
            game.add_primed_tnt(
                horizon_core::position::BlockPos::new(0, 65, 5),
                0.0,
                80,
                None,
                None,
            );
            let id = game.tnts[0].id as i32;
            let original_velocity = game.tnts[0].velocity;
            // A closer cow on the same ray, as when an entity has moved since
            // the client picked the packet's TNT target.
            let cow = game.res.mob_types().get("minecraft:cow").unwrap().clone();
            game.mobs.push(horizon_client::mobs::Mob::new(
                id as u32 + 1,
                cow,
                Vec3::new(0.5, 65.0, 4.5),
                0.0,
            ));
            let health = game.mobs[0].health;
            game.lan_guest_packet(12, S::Attack(play::Attack { id }));
            assert_eq!(game.as_guest(12, |g| g.attack_ticks).unwrap(), 0);
            let location = [0.0, 0.625, -f64::from(0.49_f32)];
            game.lan_guest_packet(
                12,
                S::Interact(play::Interact {
                    id,
                    hand: 1,
                    location,
                    secondary: false,
                }),
            );
            assert_eq!(
                *spy.0.lock().unwrap(),
                [
                    ServerNotice::Attack(id),
                    ServerNotice::InteractSpecific(id, location),
                    ServerNotice::Interact(id, Hand::Off),
                ]
            );
            assert_eq!(game.mobs[0].health, health);
            assert_eq!(game.tnts[0].fuse, 80);
            assert_eq!(game.tnts[0].velocity, original_velocity);

            spy.0.lock().unwrap().clear();
            game.tnts[0].position.z = 28.5;
            game.lan_guest_packet(12, S::Attack(play::Attack { id }));
            game.lan_guest_packet(
                12,
                S::Interact(play::Interact {
                    id,
                    hand: 1,
                    location,
                    secondary: false,
                }),
            );
            assert!(spy.0.lock().unwrap().is_empty(), "out-of-reach packets");
        });
    });
}
