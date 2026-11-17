-- The built-in announcer's script (docs/plans/announcers.md, "A pack's
-- script"): run once as its sounds are chosen, it builds its pool of
-- takes. Every sound of its voices is a take of its folder's line in its
-- folder's language (what the folders say by themselves); its languages
-- are offered alone or mixed.
local pack = Announcer.pack()
for _, file in ipairs(Announcer.files()) do
  pack:take(file)
end
pack:languages({ "en", "ja" }, { mixed = true })
return pack
