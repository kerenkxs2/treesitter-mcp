interface PanelProps { title: string }

export function panel(props: PanelProps) {
    return <section><h1>{props.title}</h1></section>;
}
