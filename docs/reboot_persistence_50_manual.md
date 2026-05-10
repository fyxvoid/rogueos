# 50-reboot persistence test (manual)

This repo does not currently have an automated QEMU expect/console driver. This describes the manual workflow for the plan’s “50+ reboots; data survives” check.

## Setup

1. Run QEMU with persistence:

```sh
system/build/run_qemu_demo.sh
```

2. In the shell, create a file:

```text
$ run editor
file: hello.txt
> hello
> exit
```

3. Verify:

```text
$ run viewer
file: hello.txt
```

## Loop (repeat 50 times)

1. Reboot:

```text
$ run shutdown
```

2. After reboot, verify file still present:

```text
$ ls
$ run viewer
file: hello.txt
```

## Notes

- `SYS_FSYNC` is implemented and exposed to userland, but the current editor implementation appends line-by-line without explicitly calling fsync (writes are synchronous block writes; superblock flush happens on reboot). If you add explicit `sys_fsync(fd)` after writes, the test becomes stronger.
- The filesystem is minimal and does not journal; “acceptable” currently means “no total corruption” and “last write may be lost if not flushed”.

