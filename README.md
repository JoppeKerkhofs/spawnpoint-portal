# spawnpoint-portal

A Minecraft launcher. `portal-core` is its library; the `portal` command-line tool drives it.

## Build

    cargo build

## Commands

    portal versions                    list the release versions, newest first
    portal files <version>             download the game files of a version
    portal java <component>            download a Java runtime, e.g. java-runtime-delta
    portal launch <version> --dir <folder> [--name <player>] [--server <host>] [--memory <MB>]
                                       [--loader <fabric|neoforge|forge>[:<version>]]
                                       start the game offline (name defaults to Player, memory to 4096;
                                       --loader adds a mod loader, newest version unless given)
