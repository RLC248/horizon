use mod_api::{
    ClientHost, DimensionTransition, DimensionTransitionCause, DimensionTransitionScreen,
    DimensionTransitionScreenDrawEvent, Hooks, RegisterDimensionTransitionScreenEvent,
    RegistrationContext,
};

const SCREEN: &str = "transition_proof:loading";

#[derive(Default)]
pub struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register_dimension_transition_screen(
        &self,
        _context: &RegistrationContext,
        event: &mut RegisterDimensionTransitionScreenEvent<'_>,
    ) {
        for dimension in ["minecraft:overworld", "minecraft:the_end"] {
            event.screens.push(DimensionTransitionScreen {
                screen: SCREEN.into(),
                transition: DimensionTransition::Incoming(dimension.into()),
            });
        }
    }

    fn dimension_transition_screen_draw(
        &self,
        host: &dyn ClientHost,
        event: &mut DimensionTransitionScreenDrawEvent<'_>,
    ) {
        if event.screen != SCREEN {
            return;
        }
        let cause = event.cause(host);
        let accent = match cause {
            Some(DimensionTransitionCause::Login) => 0xFF72C5FC,
            Some(DimensionTransitionCause::DeathRespawn) => 0xFFFBA9A5,
            Some(DimensionTransitionCause::Travel) => 0xFF9DDFAD,
            _ => 0xFFFFCE78,
        };
        let cause_text = match cause {
            Some(cause) => format!("Cause: {cause:?}"),
            None => "Cause: None".into(),
        };
        let reason_text = format!("Reason: {:?}", event.reason);
        let [width, height] = event.draw.size();
        let left = (width - 190.0).max(0.0) / 2.0;
        let top = (height - 64.0).max(0.0) / 2.0;

        event.draw.fill([0.0, 0.0, width, height], 0xFF151C2A);
        event.draw.fill([left, top, 190.0, 64.0], 0xFF202C40);
        event.draw.fill([left, top, 190.0, 2.0], accent);
        event.draw.text(event.screen, [left + 12.0, top + 11.0], 0xFFE6EBF4, true);
        event.draw.text(&cause_text, [left + 12.0, top + 28.0], accent, true);
        event.draw.text(&reason_text, [left + 12.0, top + 45.0], 0xFFE6EBF4, true);
    }
}
