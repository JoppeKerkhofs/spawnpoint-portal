# Link a panel by signing in

Linking a panel today means: Portal shows a code, the player opens the panel's API tokens page, types the code, and picks abilities from a list made for operators. That page is right for someone who runs the panel and wrong for a friend who only wants to play.

This design makes linking a normal sign-in. Portal opens the panel in the browser, the player signs in the way they always do, checks one screen and clicks Allow. The token is made for them with only what a launcher needs.

It covers two repositories: the panel (`spawnpoint`, version 1.9.4) and Portal (this one, version 0.3.0, the same release as the Microsoft sign-in).

## What the player does

1. In Portal's Panels view: type the panel's address, click **Link**.
2. The browser opens the panel. Not signed in yet: the panel's own sign-in comes first, with password, passkey and two-factor as usual, and leads back to step 3.
3. The panel shows one screen:
   - the code, the same one Portal shows, to compare;
   - what the device asks for, in words: "See your servers" and "Join your servers";
   - the device's name in a text field, filled in with "Spawnpoint Portal on \<computer name\>", free to change;
   - **Allow** and **Not my device**.
4. After Allow, Portal notices by itself and shows the panel as linked. The player sees the servers shared with their account, no others.

The player never types a code and never sees the ability picker. They still need an account on the panel; that is on purpose, see "What stays the same".

## Decisions

- **The sign-in happens in the browser, never in Portal.** Portal has no password field for a panel. Passkeys and two-factor only work on the panel's own page, and Portal should not hold a panel password at any moment.
- **Allow is always asked, also when already signed in.** Without it, a link a player clicks could link a stranger's device to their account without them noticing.
- **The code stays visible in both places.** The screen says to allow only a device that shows the same code. That is the defence against someone sending a player a link to a pairing the sender started.
- **A device can only ask for `servers:read` and `servers:join`.** Anything else is refused when the pairing starts. More than that stays behind the API tokens page, where the person picks abilities themselves.
- **The device name is set on the Allow screen.** It becomes the token's name, which is what the panel's token list and activity log show, so that is where the person who will later recognise it should choose it.
- **The old way stays.** The code field and the ability picker on the API tokens page are unchanged, for tokens made by hand and for other apps.

## What stays the same

- A token belongs to a user and never exceeds them. A friend sees and joins the servers shared with their account.
- Every friend needs a panel account and a share on a server. Inviting and sharing are unchanged.
- A panel that requires two-factor still requires it: an account without it can link, and then gets the panel's message to set it up.
- Pairings expire after ten minutes, and the token is collected once.

## Panel (1.9.4)

### Starting a pairing

`POST /api/v1/pair` takes one more, optional field:

- `abilities`: a list; each entry `servers:read` or `servers:join`; at least one. Anything else: 422.

Stored on the pairing in a new nullable column `requested_abilities` (json) of `device_pairings`.

The answer gains `approve_path` when `abilities` was sent: the path of the Allow screen for this pairing, `/pair/<code>`. A path and not an address, because the app knows which address it reached the panel at and the panel may be known under more than one. A pairing started without `abilities` behaves as today and has no `approve_path`.

### The Allow screen

Three web routes behind `auth` and `verified`, like the API tokens page:

- `GET /pair/{code}`: the screen. Laravel's own redirect to the sign-in page and back covers the player who is not signed in.
- `POST /pair/{code}`: Allow. Field `device_name`: required, at most 60 characters.
- `DELETE /pair/{code}`: Not my device. Deletes the pairing.

Rules:

- The code is matched the way the tokens page does it: upper case, spaces and dashes forgiven.
- Only a pairing that is waiting, not expired, and has `requested_abilities` gets the screen. Anything else shows one message: "No device is waiting with that code. Codes expire after ten minutes. Start again in the app."
- Allow calls the existing `DevicePairing::approve` with the requested abilities and the chosen name. The token is created under that name, and the activity log records it as it does today.
- After Allow the screen says the device is linked and the tab can be closed.
- All three routes are throttled (20 a minute per user), since a signed-in user could otherwise guess codes.

The page is a new Inertia page in the look of the panel's sign-in pages, with its texts going through the panel's translation calls.

### Not changed

`POST /api/v1/pair/token` and the API tokens page.

## Portal (0.3.0)

### Core

- `pair_start` sends `abilities: ["servers:read", "servers:join"]`.
- `Pairing` gains `approve_path: Option<String>`. It is used only when it is exactly `/pair/<this pairing's code>`, and is then put behind the address the player typed; anything else is treated as absent. A panel must not be able to make Portal open another site.
- A 403 from the panel shows the panel's own message when it is about two-factor, instead of always saying the token lacks abilities.
- A pairing that the player refused answers 404 like an expired one. The text becomes "the link was refused or the code expired".

### Window

The Panels view's link card:

- The button reads **Link** instead of "Get a code".
- With an `approve_path`: Portal opens that page in the browser at once and shows "Finish in your browser", the code to compare, the countdown, **Open the page again** and **Cancel**. The three numbered steps become: sign in to the panel, check the code, click Allow.
- Without one (a panel older than 1.9.4): exactly today's screen and steps.
- The warning about missing abilities after linking stays; it can only appear with an older panel.

### Command line

`portal panel add` prints the page to open when there is one, and today's line otherwise.

## Old and new together

| Portal | Panel | What happens |
|---|---|---|
| 0.3.0 | 1.9.4 | Sign-in link, Allow screen. |
| 0.3.0 | older | The panel ignores `abilities` and sends no `approve_path`; today's code flow. |
| older | 1.9.4 | No `abilities` sent; today's code flow. |

## Checks

Panel (Pest):

- Starting with `abilities` stores them and returns an `approve_path`; an ability outside the two is refused; starting without them is unchanged.
- The Allow screen: a guest is sent to sign in and comes back; a viewer (not an admin) can allow; an expired, unknown, already approved or hand-flow pairing shows the one message.
- Allow mints a token with exactly the requested abilities under the chosen name; a name over 60 characters is refused; Not my device deletes the pairing and the device's next poll answers 404.

Portal:

- Core: `approve_path` is used when it is this code's own path and dropped otherwise (another code, an address, a path that climbs out); the 403 message; both answers of `/pair`.
- Window: which screen shows with and without an `approve_path`.
- By hand, with a non-admin account on a real panel: link from a signed-out browser, from a signed-in one, rename the device and find that name in the panel's token list, refuse once, let a code expire once.

## Order of work

1. Panel 1.9.4, released first. Nothing in Portal depends on it being there, and it can be tried with `curl`.
2. Portal 0.3.0, on top of the Microsoft sign-in branch, so both go out in one release. The sign-in pull request is merged only when this is in it.

## Not in this design

- Linking without a panel account (invite links).
- Renaming a device after it is linked.
- Portal showing which panel user it is linked as.
