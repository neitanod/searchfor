namespace Geometry {
  export abstract class Base {
    abstract area(): number;

    describe(): string {
      return `needle ${this.area()}`;
    }
  }
}

interface Shape {
  area(): number;
}

export const make = (n: number): string => {
  return "needle".repeat(n);
};
