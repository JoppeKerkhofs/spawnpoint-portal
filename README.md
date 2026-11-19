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
