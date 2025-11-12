MEMORY
{
	image : ORIGIN = 0x40080000, LENGTH = 4M
	HEAP (rw) : ORIGIN = 0x40400000, LENGTH = 4M
	PAYLOAD (rwx) : ORIGIN = 0x40800000, LENGTH = 64M
}

SECTIONS {
     .heap :
	 {
       *(.heap);
       . = ALIGN(4);
     } > HEAP
	.payload :
	{
       *(.payload);
       . = ALIGN(4);
     } > PAYLOAD
} INSERT AFTER .text;
