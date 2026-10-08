- replays: record all moves as they are played and allow at the end of game to download them into a standardized compact text format like chess notation
- same as with active stones, when placing a stone but not having done a move, we should be allowed to still change our mind
- settings: enable/disable animations (granular), field size (not arbitrary, for now say five fixed options, current value +/- 2 for now), 
- general setup for bot support, with a basic bot for now that just picks whichever move first move flips the most fields to the bot's color, code architectured cleanly for future other bots
- general UI: replays, settings, bot selection all need UI changes, consider what a good approach would be, consider whether we should perhaps use some established bevy UI library (only if that really makes sense and doesnt mean we have to fight against it's defaults, etc)
- themes: general support for different themes. for now lets keep the current theme as one option, and then have a second option where the red/green fields are instead playing card style heart or cross symbol on white background, the stones with some faux shading to make them look like Go pieces
- keybinds: currently they capture everything, including e.g. alt+2/ctrl+2 to switch to a dfiffernt tab in browser, those should not be captured, only the ones we actually use
- the right clicking to draw lines does not seem to work at all (?). also a change in design: either drag or click, so click to start click to stop. and a pale line preview in either case
- Settings should be saved to a dotfile in native mode and to cookies in web mode

## Long Term Ideas (not to be implemented right away)

- Puzzles
- Elo, Leaderboards, Matchmaking, wow!
