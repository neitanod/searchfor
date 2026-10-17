export class Widget {
  constructor(el) {
    this.el = el;
  }

  render() {
    return `<p>needle</p>`;
  }

  handler = () => {
    console.log("needle clicked");
  };
}

const helpers = {
  format(x) {
    return x + " needle";
  },
};
