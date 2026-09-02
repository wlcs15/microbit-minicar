MEMORY
{
  /* Last 8K (two 4K pages) reserved for the calibration flash log. */
  FLASH : ORIGIN = 0x00000000, LENGTH = 504K
  RAM   : ORIGIN = 0x20000000, LENGTH = 128K
}
