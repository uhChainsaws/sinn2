target extended-remote :3333
load
break main
#monitor arm semihosting enable
continue
