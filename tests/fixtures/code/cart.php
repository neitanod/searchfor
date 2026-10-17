<?php
namespace App\Shop;

class Cart
{
    private array $items = [];

    public function addItem(Item $item, int $qty = 1): void
    {
        // needle: merge equal items
        $this->items[] = [$item, $qty];
    }
}

function needle_helper(): string
{
    return "needle";
}
