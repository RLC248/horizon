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
impl BlockPart for StorageBlock {}

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

struct PaperPouch;

#[mod_api::horizon_part]
impl ItemPart for PaperPouch {
    fn use_on(&self, host: HostSide<'_>, event: &mut UseOnEvent<'_>) -> Option<InteractionResult> {
        if event.hand != Hand::Main {
            return None;
        }
        if let HostSide::Server(host) = host {
            let player = host.player(event.player)?;
            let menu = host.menu_keys().into_iter().find(|key| key.id(host) == MENU)?;
            let window = player
                .open_mod_menu(
                    host,
                    menu,
                    Text::literal("Held paper pouch"),
                    vec![37],
                    None,
                    MenuReplace::InPlace,
                )
                .expect("open the item-backed content menu")?;
            attach_held_slots(host, player, window);
            host.messages().broadcast(Text::literal(format!(
                "Pouch opened from the held item on a click at {:?}",
                event.pos
            )));
        }
        Some(InteractionResult::Consume)
    }
}

struct BookPouch;

#[mod_api::horizon_part]
impl ItemPart for BookPouch {
    fn use_on(&self, host: HostSide<'_>, event: &mut UseOnEvent<'_>) -> Option<InteractionResult> {
        if event.hand != Hand::Main {
            return None;
        }
        if let HostSide::Server(host) = host {
            let player = host.player(event.player)?;
            let window = host
                .open_menu(
                    event.player,
                    1,
                    Text::literal("Held book pouch"),
                    Vec::new(),
                    MenuReplace::InPlace,
                )
                .expect("open the item-backed stock chest menu")?;
            attach_held_slots(host, player, window);
        }
        Some(InteractionResult::Consume)
    }
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
        registrar.item_part(paper, PaperPouch);
        let book = registrar.item_key("minecraft:book");
        registrar.item_container(&book, |_: &dyn Host| Some(2));
        registrar.item_part(book, BookPouch);
        registrar.menu_part(MENU, HeldStorage);
    }

    fn blocks(&self, _: &RegistrationContext, event: &mut BlocksEvent<'_>) {
        event.blocks.push(Block::new(MACHINE, "minecraft:gold_block"));
        event.blocks.push(Block::new(CHEST_MACHINE, "minecraft:iron_block"));
    }
}
