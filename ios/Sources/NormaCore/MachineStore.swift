import Foundation

/// Metadata only. Credentials live exclusively in CredentialVault.
public actor MachineStore {
    private static let maximumBytes = 16 * 1024 * 1024
    private let url: URL
    public init(url: URL) { self.url = url }

    public func load() throws -> [PairedMachine] {
        guard FileManager.default.fileExists(atPath: url.path) else { return [] }
        // At most 128 Macs × 256 cached project names; the file stays bounded before decoding.
        let data = try Data(contentsOf: url)
        guard data.count <= Self.maximumBytes else { throw MobileError.invalidResponse }
        let machines = try JSONDecoder().decode([PairedMachine].self, from: data)
        guard machines.count <= 128, Set(machines.map(\.id)).count == machines.count else {
            throw MobileError.invalidResponse
        }
        try machines.forEach { try $0.validate() }
        return machines
    }
    public func save(_ machine: PairedMachine) throws {
        try machine.validate()
        var machines = try load()
        if let index = machines.firstIndex(where: { $0.id == machine.id }) {
            var refreshed = machine
            let previous = machines[index]
            if let previousTime = previous.lastSyncedAt,
               previousTime > (refreshed.lastSyncedAt ?? .distantPast) {
                refreshed.lastSyncedAt = previousTime
                refreshed.knownProjects = previous.knownProjects
            }
            machines[index] = refreshed
        }
        else { machines.append(machine) }
        guard machines.count <= 128 else { throw MobileError.invalidResponse }
        try write(machines)
    }
    public func updateObservation(id: String, projects: [KnownProject], at date: Date) throws {
        var machines = try load()
        guard let index = machines.firstIndex(where: { $0.id == id }) else { throw MobileError.invalidResponse }
        if let latest = machines[index].lastSyncedAt, latest > date { return }
        machines[index].lastSyncedAt = date
        machines[index].knownProjects = projects
        try machines[index].validate()
        try write(machines)
    }
    public func remove(id: String) throws { try write(load().filter { $0.id != id }) }
    private func write(_ machines: [PairedMachine]) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let data = try JSONEncoder().encode(machines)
        guard data.count <= Self.maximumBytes else { throw MobileError.invalidResponse }
        try data.write(to: url, options: .atomic)
    }
}
