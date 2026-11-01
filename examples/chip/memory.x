/* QEMU's mps2-an385: 4 MiB of SSRAM for code at 0, 4 MiB of SSRAM for data at 0x20000000. */
MEMORY
{
  FLASH : ORIGIN = 0x00000000, LENGTH = 4M
  RAM   : ORIGIN = 0x20000000, LENGTH = 4M
}
