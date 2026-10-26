"""Same comparison layout and labels with the chosen B+D monitor light."""
from material_sheet import ROOT, sheet

if __name__=='__main__':
    sheet(ROOT/'target/scratch/keycaps/bd-materials',
          'Materials / B + D lighting',
          'Monitor 60 cm behind · 27 cm high · 15° back · faint room light')
