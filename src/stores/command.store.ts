import { create } from "zustand";
import { createSingleton } from "./singleton";

export interface Command {
  id: string;
  name: string;
  shortcut?: string;
  category: string;
  execute: () => void | Promise<void>;
}

interface CommandState {
  isOpen: boolean;
  query: string;
  commands: Command[];
  filteredCommands: Command[];
  open: () => void;
  close: () => void;
  setQuery: (q: string) => void;
  execute: (commandId: string) => Promise<void>;
  registerCommands: (cmds: Command[]) => void;
}

export const useCommandStore = createSingleton("command", () =>
  create<CommandState>((set, get) => ({
  isOpen: false,
  query: "",
  commands: [],
  filteredCommands: [],

  open: () => set({ isOpen: true, query: "", filteredCommands: get().commands }),

  close: () => set({ isOpen: false, query: "" }),

  setQuery: (q: string) => {
    const lower = q.toLowerCase();
    const filtered = lower
      ? get().commands.filter(
          (c) =>
            c.name.toLowerCase().includes(lower) ||
            c.category.toLowerCase().includes(lower),
        )
      : get().commands;
    set({ query: q, filteredCommands: filtered });
  },

  execute: async (commandId: string) => {
    const cmd = get().commands.find((c) => c.id === commandId);
    if (cmd) {
      get().close();
      await cmd.execute();
    }
  },

  registerCommands: (cmds: Command[]) =>
    set({ commands: cmds, filteredCommands: cmds }),
  })),
);
