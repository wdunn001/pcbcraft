import sys
sys.path.insert(0,".")
from check_fd import microstrip_fd
import models as M
h,er=1.6,4.5
for div in (8,16,32):
    z,_=microstrip_fd(1.0*h,h,er,dx=h/div)
    r=microstrip_fd(1.0*h,h,er,s=0.25*h,dx=h/div)
    print(div,"single",round(z,2),"coupled ze,zo",round(r[0],2),round(r[1],2),flush=True)
print("HJ",M.microstrip_nothick(h,h,er)[0],M.coupled(h,0.25*h,h,er)[:2])
