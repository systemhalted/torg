import SwiftUI

// The torg iPad/iOS spike. Everything structural (parsing the outline, cycling a TODO) is
// computed by torg-core across the UniFFI bridge — this Swift layer only renders and handles
// touch. See ContentView for where the Rust functions are called.
@main
struct TorgSpikeApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}
