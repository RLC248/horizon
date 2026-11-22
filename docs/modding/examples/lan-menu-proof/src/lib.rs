use mod_api::{
    Block, BlockEntityPart, BlockPart, BlocksEvent, ClientHost, Hand, Hooks, Host, HostSide,
    InteractionResult, ItemPart, MenuPart, MenuReplace, MenuScreenEvent, MenuSlotPlace,
    ModContainer, PlayerRef, Registrar, RegistrationContext, Screen, ScreenEvent, ServerHost,
    Text, UseOnEvent,
};

const MACHINE: &str = "lan_menu_proof:machine";
const MENU: &str = "lan_menu_proof:storage";
const CHEST_MACHINE: &str = "lan_menu_proof:chest_machine";
const CHEST_MENU: &str = "minecraft:generic_9x1";

struct StorageBlock;

#[mod_api::horizon_part]
impl BlockPart for StorageBlock {
    fn use_without_item(
        &self,
        _: &mut dyn ServerHost,
        event: &mut mod_api::UseWithoutItemEvent<'_>,
    ) -> Option<InteractionResult> {
        OPENS.lock().unwrap().push(OpenRequest {
            player: event.click.player,
            pos: event.click.pos,
            kind: OpenKind::Block,
        });
        Some(InteractionResult::Consume)
    }
}

#[mod_api::horizon_part]
impl BlockEntityPart for StorageBlock {
    fn screen(&self, _: &dyn Host, event: &mut ScreenEvent<'_>) -> Option<Screen> {
        if event.block != MACHINE && event.block != CHEST_MACHINE {
            return None;
        }
        Some(Screen {
            width: 176.0,
            height: 165.0,
            slots: vec![Some([35.0, 41.0]), None],
            inventory_x: 8.0,
            inventory_y: 83.0,
            ..Default::default()
        })
    }
}

struct HeldStorage;

#[mod_api::horizon_part]
impl MenuPart for HeldStorage {
    fn menu_screen(&self, _: &dyn ClientHost, event: &mut MenuScreenEvent<'_>) -> Option<Screen> {
        if event.menu != MENU {
            return None;
        }
        Some(Screen {
            width: 196.0,
            height: 154.0,
            slots: vec![Some([29.0, 25.0]), Some([130.0, 42.0])],
            inventory_x: 18.0,
            inventory_y: 72.0,
            ..Default::default()
        })
    }
}

#[derive(Clone, Copy)]
enum OpenKind {
    Block,
    Paper,
    Book,
}

struct OpenRequest {
    player: u128,
    pos: [i32; 3],
    kind: OpenKind,
}

static OPENS: std::sync::Mutex<Vec<OpenRequest>> = std::sync::Mutex::new(Vec::new());

struct PouchItem(OpenKind);

#[mod_api::horizon_part]
impl ItemPart for PouchItem {
    fn use_on(&self, host: HostSide<'_>, event: &mut UseOnEvent<'_>) -> Option<InteractionResult> {
        if event.hand != Hand::Main {
            return None;
        }
        if matches!(host, HostSide::Server(_)) {
            // The click runs with the guest's inventory loaned into the world.
            // Open only after the server has returned it to the guest record.
            OPENS.lock().unwrap().push(OpenRequest {
                player: event.player,
                pos: event.pos,
                kind: self.0,
            });
        }
        Some(InteractionResult::Consume)
    }
}

fn open_requested_menu(host: &mut dyn ServerHost, request: OpenRequest) -> Option<()> {
    let player = host.player(request.player)?;
    let window = match request.kind {
        OpenKind::Block => {
            let level = host.level("minecraft:overworld")?;
            let block = host.block_entity(level, request.pos)?;
            assert!(player.open_menu(host, block).expect("open the actual block entity menu"));
            return Some(());
        }
        OpenKind::Paper => {
            let menu = host.menu_keys().into_iter().find(|key| key.id(host) == MENU)?;
            player
                .open_mod_menu(
                    host,
                    menu,
                    Text::literal("Held paper pouch"),
                    vec![37],
                    None,
                    MenuReplace::InPlace,
                )
                .expect("open the item-backed content menu")?
        }
        OpenKind::Book => host
            .open_menu(
                request.player,
                1,
                Text::literal("Held book pouch"),
                Vec::new(),
                MenuReplace::InPlace,
            )
            .expect("open the item-backed stock chest menu")?,
    };
    attach_held_slots(host, player, window);
    host.messages().broadcast(Text::literal(format!(
        "Pouch opened from the held item on a click at {:?}",
        request.pos
    )));
    Some(())
}

fn attach_held_slots(host: &mut dyn ServerHost, player: PlayerRef, window: i32) {
    let places = (0..2)
        .map(|slot| MenuSlotPlace::HeldItem {
            hand: Hand::Main,
            slot,
        })
        .collect();
    player
        .back_mod_menu_slots(host, window, places)
        .expect("attach both slots to the held item");
}

#[derive(Default)]
pub struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, registrar: &mut dyn Registrar, _: &RegistrationContext) {
        let kind = registrar
            .block_entity_type("storage")
            .blocks(&[MACHINE, CHEST_MACHINE])
            .build();
        registrar.block_entity_part(&kind, StorageBlock).unwrap();

        for (id, slots, menu) in [(MACHINE, 2, MENU), (CHEST_MACHINE, 9, CHEST_MENU)] {
            let block = registrar.block_key(id);
            registrar.block_container(
                &block,
                ModContainer {
                    slots,
                    menu: Some(menu.into()),
                    ..Default::default()
                },
            );
        }

        let paper = registrar.item_key("minecraft:paper");
        registrar.item_container(&paper, |_: &dyn Host| Some(2));
        registrar.item_part(paper, PouchItem(OpenKind::Paper));
        let book = registrar.item_key("minecraft:book");
        registrar.item_container(&book, |_: &dyn Host| Some(2));
        registrar.item_part(book, PouchItem(OpenKind::Book));
        registrar.menu_part(MENU, HeldStorage);
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut mod_api::ServerTickEvent<'_>) {
        if matches!(event.phase, mod_api::ServerTickPhase::ServerTickEnd { .. }) {
            let requests = std::mem::take(&mut *OPENS.lock().unwrap());
            for request in requests {
                let _ = open_requested_menu(host, request);
            }
        }
    }

    fn blocks(&self, _: &RegistrationContext, event: &mut BlocksEvent<'_>) {
        event.blocks.push(Block::new(MACHINE, "minecraft:gold_block"));
        event.blocks.push(Block::new(CHEST_MACHINE, "minecraft:iron_block"));
    }
}
