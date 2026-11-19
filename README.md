# spawnpoint-portal

A Minecraft launcher. `portal-core` is its library; the `portal` command-line tool drives it.

## Build

    cargo build

## Commands

    portal versions                    list the release versions, newest first
    portal files <version>             download the game files of a version
    portal java <component>            download a Java runtime, e.g. java-runtime-delta
    portal instance list               id, name, version, loader and source of each instance
    portal instance create <name> <version> [--loader <fabric|neoforge|forge>[:<version>]]
                                       the loader's newest version is stored when none is given
    portal instance delete <id>
    portal instance mods <id>          name, size, pack or own
    portal instance add-mod <id> <jar>
    portal instance remove-mod <id> <file name>
    portal pack install ftb <pack> <version> [--name <name>]
    portal pack install modrinth <version id> [--name <name>]
                                       make an instance from a pack; files to fetch by hand are listed
    portal pack update <id> <version>  install another version of the instance's pack over it
    portal launch <id> [--name <player>] [--server <host>] [--memory <MB>]
                                       start the game offline (name defaults to Player, memory to 4096),
                                       wait for it to exit
    portal panel add <address> [--allow-http]
                                       link a Spawnpoint panel: approve the code it prints at
                                       <address>/settings/api-tokens (the token goes in the
                                       Windows Credential Manager, never in a file)
    portal panel list                  id, name and address of each linked panel
    portal panel remove <id>           unlink a panel and forget its token
    portal servers                     <panel id>/<server>, name, state, version, loader or pack
    portal sync <panel id>/<server>    make the server's instance match the server
    portal join <panel id>/<server> [--name <player>]
                                       sync, then start the game on that server; the panel id
                                       may be left out when one panel is linked
