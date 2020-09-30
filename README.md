# Sayler n-collision finder

This program finds an MD5 Sayler n-collision. An MD5 Sayler n-collision is defined as a pair of distinct inputs whose MD5 hash matches in the first n and last n printed hex characters. 

For example this is a Sayler-6 Collision:
```
$ md5sum file1
d41d8ce1987fbb152380234511f8427e  file1

$ md5sum file2
d41d8cd98f00b204e9800998ecf8427e  file2
```

The program takes a single required positional argument, n. A parallel implementation can be run using the -p flag. The -h flag prints out a help message with more details about allowed arguments.

This work was done as a class homework assignment.
