use mod_api::{
    Block, BlockEntityPart, BlockPart, BlocksEvent, ClientHost, Hand, Hooks, Host, HostSide,
    InteractionResult, ItemPart, MenuPart, MenuReplace, MenuScreenEvent, MenuSlotPlace,
    ModContainer, Registrar, RegistrationContext, Screen, ScreenEvent, Text, UseOnEvent,
};

const MACHINE: &str = "lan_menu_proof:machine";
const MENU: &str = "lan_menu_proof:storage";

struct StorageBlock;

#[mod_api::horizon_part]
impl BlockPart for StorageBlock {}

#[mod_api::horizon_part]
impl BlockEntityPart for StorageBlock {
    fn screen(&self, _: &dyn Host, event: &mut ScreenEvent<'_>) -> Option<Screen> {
        if event.block != MACHINE {
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
            let places = (0..2)
                .map(|slot| MenuSlotPlace::HeldItem {
                    hand: Hand::Main,
                    slot,
                })
                .collect();
            player
                .back_mod_menu_slots(host, window, places)
                .expect("attach both slots to the held paper");
            host.messages().broadcast(Text::literal(format!(
                "Pouch opened from the held item on a click at {:?}",
                event.pos
            )));
        }
        Some(InteractionResult::Consume)
    }
}

#[derive(Default)]
pub struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, registrar: &mut dyn Registrar, _: &RegistrationContext) {
        let kind = registrar
            .block_entity_type("storage")
            .blocks(&[MACHINE])
            .build();
        registrar.block_entity_part(&kind, StorageBlock).unwrap();

        let block = registrar.block_key(MACHINE);
        registrar.block_container(
            &block,
            ModContainer {
                slots: 2,
                menu: Some(MENU.into()),
                ..Default::default()
            },
        );

        let paper = registrar.item_key("minecraft:paper");
        registrar.item_container(&paper, |_: &dyn Host| Some(2));
        registrar.item_part(paper, PaperPouch);
        registrar.menu_part(MENU, HeldStorage);
    }

    fn blocks(&self, _: &RegistrationContext, event: &mut BlocksEvent<'_>) {
        event.blocks.push(Block::new(MACHINE, "minecraft:gold_block"));
    }
}
