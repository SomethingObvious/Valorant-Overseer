// ink imports react-devtools-core at module load but only calls it when
// DEV=true, which this build never sets. Bundling the real package would add
// megabytes of debugger for a branch that never runs.
export default {
  connectToDevTools(): void {
    // Intentionally nothing.
  },
};
