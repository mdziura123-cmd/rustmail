import React from "react";

interface State {
  error: Error | null;
}

/** Zapobiega białemu ekranowi: błąd wyświetlania pokazuje komunikat i pozwala wrócić do aplikacji. */
export class ErrorBoundary extends React.Component<{ children: React.ReactNode }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error("Błąd wyświetlania:", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div role="alert" style={{ padding: 24, font: "14px system-ui, sans-serif" }}>
        <h2>Coś poszło nie tak przy wyświetlaniu</h2>
        <p>Poczta jest bezpieczna – to błąd tylko w interfejsie. Możesz wrócić do aplikacji.</p>
        <pre style={{ whiteSpace: "pre-wrap", opacity: 0.8 }}>{String(this.state.error.message).slice(0, 500)}</pre>
        <button onClick={() => this.setState({ error: null })}>Wróć do aplikacji</button>{" "}
        <button onClick={() => window.location.reload()}>Uruchom interfejs ponownie</button>
      </div>
    );
  }
}
