# Grex

Grex is a CLI tool that makes XML greppable. It's like [gron](https://github.com/tomnomnom/gron) but for XML instead of JSON.

Grex transforms XML into a flat, line-oriented representation that is easy to use with UNIX text processing tools like grep, sed, awk, diff, etc. The transformation is reversible, so you can convert back to XML at the end.

## Example

Given this XML:
```xml
<pizzeria name="Panucci's Pizza" rating="2.3">
  <location>
    <address>1020 West 57th Street, New York, NY</address>
    <phone>(212) 555-PIZZA</phone>
  </location>
  <menu>
    <pizza name="Cheese" price="14.99"/>
    <pizza name="Pepperoni" price="15.99"/>
    <pizza name="Anchovy Special" price="17.99"/>
  </menu>
</pizzeria>
```

Grex produces:
```
/pizzeria/@name = Panucci's Pizza
/pizzeria/@rating = 2.3
/pizzeria/location/address/text() = 1020 West 57th Street, New York, NY
/pizzeria/location/phone/text() = (212) 555-PIZZA
/pizzeria/menu/pizza[1]/@name = Cheese
/pizzeria/menu/pizza[1]/@price = 14.99
/pizzeria/menu/pizza[2]/@name = Pepperoni
/pizzeria/menu/pizza[2]/@price = 15.99
/pizzeria/menu/pizza[3]/@name = Anchovy Special
/pizzeria/menu/pizza[3]/@price = 17.99
```

Each line starts with an [XPath](https://en.wikipedia.org/wiki/XPath) expression that describes a location in the XML document tree. After that is an `=` sign and then the value of the attribute or text node at that location.

This flattened representation makes it easy to search and manipulate the data. For example, you can `grep` to find all of the prices:

```
$ grex pizzeria.xml | grep '@price'
/pizzeria/menu/pizza[1]/@price = 14.99
/pizzeria/menu/pizza[2]/@price = 15.99
/pizzeria/menu/pizza[3]/@price = 17.99
```

You can use `grex --ungrex` to reverse the transformation and reconstruct XML. This makes it possible to use grex for modifying XML: flatten the tree, filter or modify it with UNIX tools, then convert it back to XML again. (Tip: `alias ungrex="grex --ungrex"`)

Say you want to raise all of the menu prices by 10%. You can flatten the XML with grex, modify the `@price` lines with awk, and then ungrex to get back to XML.

```
$ grex pizzeria.xml \
  | awk -F' = ' '/@price/ {printf "%s = %.2f\n", $1, $2 * 1.10; next} {print}' \
  | grex --ungrex
```
```xml
<?xml version="1.0" encoding="UTF-8"?>
<pizzeria name="Panucci's Pizza" rating="2.3">
  <location>
    <address>1020 West 57th Street, New York, NY</address>
    <phone>(212) 555-PIZZA</phone>
  </location>
  <menu>
    <pizza name="Cheese" price="16.49"/>
    <pizza name="Pepperoni" price="17.59"/>
    <pizza name="Anchovy Special" price="19.79"/>
  </menu>
</pizzeria>
```

A useful property of the `grex` format is that you can merge two documents by concatenating them. So to add a new pizza to the menu, you can do something like this:

```
$ cat <<EOF
/pizzeria/menu/pizza[4]/@name = Veggie Supreme
/pizzeria/menu/pizza[4]/@price = 16.99
EOF > menu_updates.grex
```

```
$ grex pizzeria.xml | cat - menu_updates.grex | grex --ungrex
```

```xml
<?xml version="1.0" encoding="UTF-8"?>
<pizzeria name="Panucci's Pizza" rating="2.3">
  <location>
    <address>1020 West 57th Street, New York, NY</address>
    <phone>(212) 555-PIZZA</phone>
  </location>
  <menu>
    <pizza name="Cheese" price="16.49"/>
    <pizza name="Pepperoni" price="17.59"/>
    <pizza name="Anchovy Special" price="19.79"/>
    <pizza name="Veggie Supreme" price="16.99"/>
  </menu>
</pizzeria>
```

The line-oriented format is also useful for diffing:

```
$ diff <(grex pizzeria_old.xml) <(grex pizzeria_new.grex)
```
```diff
6c6
< /pizzeria/menu/pizza[1]/@price = 14.99
---
> /pizzeria/menu/pizza[1]/@price = 16.49
8c8
< /pizzeria/menu/pizza[2]/@price = 15.99
---
> /pizzeria/menu/pizza[2]/@price = 17.59
10c10,12
< /pizzeria/menu/pizza[3]/@price = 17.99
---
> /pizzeria/menu/pizza[3]/@price = 19.79
> /pizzeria/menu/pizza[4]/@name = Veggie Supreme
> /pizzeria/menu/pizza[4]/@price = 16.99
```

## Limitations

- The XML -> Grex -> XML roundtrip is mostly lossless, but CDATA and comments are discarded.
- Currently, both `grex` and `ungrex` build a DOM representation of the entire XML document in memory. In theory it'd be possible to make `grex` work using a streaming SAX-style parser, but the `ungrex` direction needs a DOM representation to work.

## See Also

- [gron](https://github.com/tomnomnom/gron) was the direct inspiration for Grex; it operates on JSON instead of XML
- [xpath-cli](https://github.com/jake-low/xpath-cli) evaluates XPath selectors on XML files; you can use it to run the path expressions that Grex outputs, or as an alternative tool for structural searches in XML data

## License

This code is available under the ISC License. See the [LICENSE](./LICENSE) file for details.
