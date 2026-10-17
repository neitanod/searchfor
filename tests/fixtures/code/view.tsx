export function View(props: { title: string }) {
  const onClick = () => {
    console.log("needle");
  };
  return <button onClick={onClick}>{props.title} needle</button>;
}
