import { run_command } from "../src/lib.js";
import { create_project } from "./helpers.js";

describe("track.cleanup", () => {
    test("reports two changes on dangle and collinear fixture", () => {
        let project = create_project();

        // Create a track with a dangling stub (one end not connected to pad or other track)
        run_command(project, "track.add", {
            net: "NET1",
            layer: "top",
            width: 0.25,
            points: [{ x: 0, y: 0 }, { x: 10, y: 0 }],
        });

        // Dangling stub: connects to main track at (5,0) but other end is free
        run_command(project, "track.add", {
            net: "NET1",
            layer: "top",
            width: 0.25,
            points: [{ x: 5, y: 0 }, { x: 5, y: 5 }],
        });

        // Two collinear segments on same net/layer that should merge
        run_command(project, "track.add", {
            net: "NET2",
            layer: "top",
            width: 0.25,
            points: [{ x: 20, y: 0 }, { x: 25, y: 0 }],
        });
        run_command(project, "track.add", {
            net: "NET2",
            layer: "top",
            width: 0.25,
            points: [{ x: 25, y: 0 }, { x: 30, y: 0 }],
        });

        let result = run_command(project, "track.cleanup", {});
        expect(result.changes).toBe(2);
    });

    test("leaves revision unchanged when nothing to do", () => {
        let project = create_project();

        // Clean track with no issues
        run_command(project, "track.add", {
            net: "NET1",
            layer: "top",
            width: 0.25,
            points: [{ x: 0, y: 0 }, { x: 10, y: 0 }],
        });

        let before = project.revision;
        let result = run_command(project, "track.cleanup", {});
        expect(result.changes).toBe(0);
        expect(project.revision).toBe(before);
    });
});
