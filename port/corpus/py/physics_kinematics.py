# Top-level constants, symplectic projectile integration, and energy conservation.
G: float = 9.80665
DT: float = 0.1

def kinetic_energy(mass: float, vx: float, vy: float) -> float:
    return 0.5 * mass * (vx * vx + vy * vy)

def potential_energy(mass: float, y: float) -> float:
    return mass * G * y

def main():
    mass: float = 2.0
    y: float = 50.0
    vx: float = 12.0
    vy: float = 15.0
    for step in range(0, 10):
        vy_half: float = vy - 0.5 * G * DT
        y += vy_half * DT
        vy = vy_half - 0.5 * G * DT
        e: float = kinetic_energy(mass, vx, vy) + potential_energy(mass, y)
        if step % 3 == 0:
            print(f"step={step} y={y:.6f} e={e:.6f}")

if __name__ == "__main__":
    main()
