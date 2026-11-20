# Window run-through

Checks that need a person looking at the real window (`target\release\spawnpoint-portal.exe`). Each line is one action and what must be seen. Items marked (dev) need `npm run tauri dev` for the developer console.

The window uses your real data: linked panels, instances and worlds. Checks marked **Destructive** say what they really do; use a throwaway instance for those.

## Everywhere

1. Start the window with the network off. It must open at once, not wait for anything.
2. In each look (standard light, standard dark, blocky), open each view and compare it with its board in `docs/superpowers/specs/window-design/` (`PortalServers`, `PortalInstances`, `PortalInstance`, `PortalPanels`, `PortalAccount` and the `PortalBlocky...` boards). Nothing may differ in layout, colour or copy, apart from the Appearance card, which has no board.
3. In Account, click each choice in the Appearance card (Standard, Blocky, Light, Dark, Follow Windows). The window must change at once, without a restart.
4. Close the window and start it again in each look. The first frame must already be the chosen look, with no flash of another one.
5. Drag the title bar. The window must move.
6. Double-click the title bar. The window must maximise; double-click again and it must restore.
7. Click each of the three title bar buttons (minimise, maximise, close). Each must do what its icon says.
8. Press Tab from the start. Focus must move through the title bar, the sidebar, the account link, the view's head, the content and the status bar's Close, in that order, and must be visible on every stop.
9. In blocky, Tab onto a button inside a grey card and inside a dialog. The focus ring must be clearly visible there.
10. Switch views with the sidebar. Focus must land on the new view's heading (a screen reader announces it).
11. With "Show animations" switched off in Windows (Settings, Accessibility, Visual effects), open the window. Nothing may move except the spinner.

## Small window (900 by 600)

12. Resize to 900 by 600. The sidebar must narrow to icons only, and each icon must show a tooltip with its name.
13. In Servers and Instances at 900 by 600, long names must be cut with an ellipsis and show the full text as a tooltip. Nothing may scroll sideways.
14. Open an instance at 900 by 600. The head's buttons must wrap onto a second line without overlapping, and the Mods card's head must wrap sensibly.
15. At 900 by 600, scroll an instance. Only the content may scroll; the sidebar, head and status bar must stay in place.
16. Open an instance with a pack of about 300 mods. The list must scroll inside the content and stay smooth, with the sidebar, head and status bar fixed.

## Servers

17. Open Servers with the network on. Each linked panel must show its servers with state, players, version and what each runs. The summary above must count servers and running servers over every panel.
18. Press Refresh. The button must be disabled while it works, and the list must stay visible.
19. Turn the network off, then open Servers. It must name each panel that cannot be reached, with a Try again button. Open Instances at the same time: it must still list your instances.
20. Click Join on a pack server (this syncs and starts the game on your machine). The status bar must show the steps ("Step i of n" with the stage), then "Starting Minecraft for ...", then "Playing ...". The server's row must change to Playing.
21. Double-click Join quickly. Only one sync may run.
22. During a join, go to another view and back. The row must still say "Preparing…".
23. Open Instances during the same join. The new instance's row must show Preparing…, and after the start, Game running.
24. During a first join of a server (one that has no instance yet), open the new instance's page and click Delete, then confirm. **Destructive**: it would delete that instance's folder. It must be refused with a message that the instance is being prepared; nothing may be deleted.
25. Join a server that is already running in this session. Its Join button must say Playing and be disabled.

## Instances

26. Open Instances. Every row must show name, version, loader, where it comes from, Open and Play. Instances Portal cannot read must be listed at the end with Show folder.
27. Click Show folder on one of them. Explorer must open on that folder.
28. Click New instance. The form must open with focus in Name. Fill it in and click Create. The new instance must appear in the list, and focus must return to the New instance button.
29. Open the form and click Cancel. The form must close and focus must return to the New instance button.
30. Click Play on an instance. The status bar must show the steps, Starting, then Playing, and the row must show Game running.

## An instance

31. Open an instance. The head must show its name and summary; the Mods card must list own mods first, then pack mods, each group in alphabetical order regardless of capitals.
32. Change Memory. The new value must stay after leaving and re-entering the page.
33. Click Mods folder. Explorer must open on the instance's mods folder.
34. Use the filter buttons (All, Pack, Own) and type in the search. The list and the counts must follow.
35. Click Add a mod file and pick a .jar. It must appear in the list as Own. **Destructive** only to the extent that it copies a file into the instance's mods folder.
36. Click Remove on one of your own mods and confirm. **Destructive**: it deletes that .jar from the instance's mods folder, and no sync restores it. Pick a mod you added in the previous step.
37. While the game of that instance is running, click Remove on an own mod and confirm. The error must read that the instance is running, and the file must stay.
38. Click Remove on a mod, then press Escape. Nothing must be removed, and focus must return to that Remove button. Repeat with Cancel.
39. Click Delete, then press Escape. Nothing must be deleted and focus must return to the Delete button. Repeat with Cancel.
40. With the mod list still loading (or after its load failed), click Delete. The dialog must not say "0 mods". It must say "its mods".
41. Click Delete and confirm on a throwaway instance. **Destructive**: it deletes the whole instance folder, including its worlds, configs and mods; it cannot be undone. The window must return to Instances, and the deleted instance must already be gone from the list.
42. Open an instance that has a file to fetch by hand, in blocky. The note's heading ("... to download by hand") must be dark and readable on the yellow note.
43. On that instance, click Open its page. The page must open in your browser, and only if its address starts with https.
44. Click Check again with the file still missing, then with it placed in the mods folder. The note must stay, then change to "All files are in place" with a green check icon.
45. Open an instance with a skipped file. The note must name the file and say your own file is in its place.
46. After a game closes, the status bar must return to "Ready" and the instance's page must show "Last played ..." with the length of the session.

## Panels

47. Open Panels. Each linked panel must show its name, its address and "Connected · n servers" (or the error when it cannot be reached).
48. In Link a panel, type an address and click Get a code. Focus must move to the new step, and a large code must appear with a countdown.
49. Click Open the approval page, check that the panel shows the same code, and approve it. The window must update by itself to "Linked: ...", and the panel must appear in the list above. **Destructive**: this stores a new key in the Credential Manager and adds a panel; remove it afterwards if it is a test.
50. Get a code and click Cancel. The form must come back, with focus in it.
51. Get a code and let it expire without approving. The window must say the code expired or was already used, with a Get a new code button.
52. Get a code, then turn the network off during the wait. It must say "The panel cannot be reached. Portal keeps trying." and must not say the code expired. Turn the network on and approve; it must link.
53. Click Get a code, then switch to another view before the code appears, and switch back. Panels must show the form, not a code or a countdown, and no countdown may keep running.
54. Click Remove on a panel, then press Escape. Nothing must be removed and focus must return to that Remove button. Repeat with Cancel.
55. Click Remove on a test panel and confirm. **Destructive**: it unlinks the panel and deletes its key from the Windows Credential Manager. Its servers disappear from Servers; instances made from it stay. Linking it again needs a new approval. Do not do this on your real panel unless you accept linking it again.

## Account

56. Change the player name and save. The sidebar footer and the next join must use the new name; invalid names must be refused with a message.
57. The Microsoft account card must show "Coming later" with its button disabled.

## Status bar and errors

58. Cause a failure (for example Play with the network off on an instance whose Java runtime is not installed). The status bar must show the error and a Close button; Narrator must read it once.
59. Click Close. The status bar must return to ready and Play and Join must work again.
60. With a game running, leave and re-enter each view. The status bar's trailing text must follow the view: since time and host in Servers and Instances, memory in the instance's page, and "as {name}" in Account.

## Developer console (dev)

61. In `npm run tauri dev`, open the webview's developer console (right-click, Inspect). Use every view. No content-security-policy or permission error may appear.
62. In the console's network or IPC log, look at the answers of `panels`, `panel_pair_start`, `panel_pair_poll` and `servers`. None may contain a token, a pairing secret or a bearer value.
63. If you have a pack that lists a `file:` or `http:` page for a hand-fetch file: its "Open its page" button must not appear.
