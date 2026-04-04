# Demo flow (plan Section 8.3)

1. Boot:

```sh
system/build/run_qemu_demo.sh
```

2. Verify WM is visible; shell prompt appears.

3. Open 3 terminals (spawns 3 shells):

```text
$ run shell
$ run shell
```

4. Create/edit file:

```text
$ run editor
file: demo.txt
> hello from demo
> exit
```

5. View file:

```text
$ run viewer
file: demo.txt
```

6. Reboot:

```text
$ run shutdown
```

7. After reboot, verify persistence:

```text
$ ls
$ run viewer
file: demo.txt
```

