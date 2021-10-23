import pyqsh
import numpy as np
import matplotlib.pyplot as plt

file = "../data/zerich/Si-3.20.2020-03-17.OrdLog.qsh"
depth = 5
lob = pyqsh.lob(file, depth)

print(type(lob))
print(lob.shape)

timestamp = lob[:,0]
mid_price = (lob[:,1] + lob[:,3]) * 0.5

plt.plot(mid_price)
plt.show()
