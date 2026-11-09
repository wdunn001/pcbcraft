"""Finite-difference cross-section solver used as a physical oracle.

Solves Laplace's equation on a grounded box with piecewise permittivity and
perfect conductors, then returns capacitance per unit length. Square cells,
node-based, harmonic-free (arithmetic edge permittivity)."""
import numpy as np
import scipy.sparse as sp
import scipy.sparse.linalg as sla

C0 = 299792458.0
EPS0 = 8.8541878128e-12


def capacitance(nx, ny, epsc, conductors, volts):
    """epsc: (nx-1, ny-1) cell permittivity. conductors: list of bool (nx, ny)
    masks. volts: potential per conductor. Box edges are grounded.
    Returns charge (per unit eps0, per metre) on each conductor."""
    fixed = np.zeros((nx, ny), bool)
    v = np.zeros((nx, ny))
    fixed[0, :] = fixed[-1, :] = True
    fixed[:, 0] = fixed[:, -1] = True
    for m, val in zip(conductors, volts):
        fixed |= m
        v[m] = val
    # edge permittivities
    ex = np.zeros((nx - 1, ny))  # edge (i,j)-(i+1,j)
    for j in range(ny):
        lo = epsc[:, j - 1] if j > 0 else None
        hi = epsc[:, j] if j < ny - 1 else None
        if lo is None:
            ex[:, j] = hi
        elif hi is None:
            ex[:, j] = lo
        else:
            ex[:, j] = 0.5 * (lo + hi)
    ey = np.zeros((nx, ny - 1))  # edge (i,j)-(i,j+1)
    for i in range(nx):
        lo = epsc[i - 1, :] if i > 0 else None
        hi = epsc[i, :] if i < nx - 1 else None
        if lo is None:
            ey[i, :] = hi
        elif hi is None:
            ey[i, :] = lo
        else:
            ey[i, :] = 0.5 * (lo + hi)
    idx = -np.ones((nx, ny), int)
    free = ~fixed
    idx[free] = np.arange(free.sum())
    n = free.sum()
    rows, cols, vals = [], [], []
    rhs = np.zeros(n)
    diag = np.zeros(n)

    def add(i0, j0, i1, j1, e):
        a = idx[i0, j0]
        if a < 0:
            return
        diag[a] += e
        b = idx[i1, j1]
        if b >= 0:
            rows.append(a)
            cols.append(b)
            vals.append(-e)
        else:
            rhs[a] += e * v[i1, j1]

    for i in range(nx - 1):
        for j in range(ny):
            e = ex[i, j]
            add(i, j, i + 1, j, e)
            add(i + 1, j, i, j, e)
    for i in range(nx):
        for j in range(ny - 1):
            e = ey[i, j]
            add(i, j, i, j + 1, e)
            add(i, j + 1, i, j, e)
    A = sp.coo_matrix((vals, (rows, cols)), shape=(n, n)).tocsr() + sp.diags(diag)
    sol = sla.spsolve(A.tocsc(), rhs)
    v[free] = sol
    out = []
    for m in conductors:
        q = 0.0
        ii, jj = np.nonzero(m)
        for i, j in zip(ii, jj):
            for (di, dj, e) in (
                (1, 0, ex[i, j] if i < nx - 1 else 0),
                (-1, 0, ex[i - 1, j] if i > 0 else 0),
                (0, 1, ey[i, j] if j < ny - 1 else 0),
                (0, -1, ey[i, j - 1] if j > 0 else 0),
            ):
                i2, j2 = i + di, j + dj
                if 0 <= i2 < nx and 0 <= j2 < ny and not m[i2, j2]:
                    q += e * (v[i, j] - v[i2, j2])
        out.append(q)
    return out
